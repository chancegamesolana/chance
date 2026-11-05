//! Pure arithmetic for entries, fees and winner selection. Every operation is checked; intermediates use u128.

use anchor_lang::prelude::*;

use crate::{constants::*, error::JackpotError};

/// Whole entries bought by `value` lamports, rounded down.
pub fn entries_for(value: u64, lamports_per_entry: u64) -> Result<u64> {
    value
        .checked_div(lamports_per_entry)
        .ok_or_else(|| error!(JackpotError::MathOverflow))
}

/// Fee on `amount` at `fee_bps`, rounded down so rounding favours the player.
pub fn fee_amount(amount: u64, fee_bps: u16) -> Result<u64> {
    let fee = u128::from(amount)
        .checked_mul(u128::from(fee_bps))
        .and_then(|v| v.checked_div(u128::from(BPS_DENOMINATOR)))
        .ok_or(JackpotError::MathOverflow)?;
    u64::try_from(fee).map_err(|_| error!(JackpotError::MathOverflow))
}

/// Picks the winning entry `random_value % total_entries` and returns `(deposit index, winning entry)`:
/// the first deposit whose running `cumulative_entries` exceeds the winning entry.
pub fn winning_index(
    cumulative_entries: impl IntoIterator<Item = u64>,
    total_entries: u64,
    random_value: u64,
) -> Result<(usize, u64)> {
    let winning_entry = random_value
        .checked_rem(total_entries)
        .ok_or(JackpotError::InconsistentRoundState)?;
    let index = cumulative_entries
        .into_iter()
        .position(|cumulative| cumulative > winning_entry)
        .ok_or(JackpotError::InconsistentRoundState)?;
    Ok((index, winning_entry))
}

/// Same rule as `winning_index` (first deposit whose running total exceeds `random_value % total_entries`),
/// found by binary search over `len` running totals read through `cumulative(i)`. Running totals never
/// decrease (every deposit is worth at least one entry), so the predicate `cumulative(i) > winning_entry` is
/// monotone and the first index where it holds is what the linear scan finds. O(log len) reads.
pub fn winning_index_sorted(
    len: usize,
    cumulative: impl Fn(usize) -> u64,
    total_entries: u64,
    random_value: u64,
) -> Result<(usize, u64)> {
    let winning_entry = random_value
        .checked_rem(total_entries)
        .ok_or(JackpotError::InconsistentRoundState)?;
    let (mut lo, mut hi) = (0usize, len);
    while lo < hi {
        // lo + (hi - lo) / 2 cannot overflow.
        let mid = lo + (hi - lo) / 2;
        if cumulative(mid) > winning_entry {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    require!(lo < len, JackpotError::InconsistentRoundState);
    Ok((lo, winning_entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn entries_floor_lamports() {
        assert_eq!(entries_for(1_000_000_000, 1).unwrap(), 1_000_000_000);
        assert_eq!(entries_for(12_345_678, 100_000).unwrap(), 123);
        assert_eq!(entries_for(99_999, 100_000).unwrap(), 0);
    }

    #[test]
    fn entries_reject_zero_lamports_per_entry() {
        assert!(entries_for(1, 0).is_err());
    }

    #[test]
    fn fee_rounds_down() {
        // 3.5% of 2_000_000_001 = 70_000_000.035
        assert_eq!(fee_amount(2_000_000_001, 350).unwrap(), 70_000_000);
        assert_eq!(fee_amount(28, 350).unwrap(), 0);
        assert_eq!(fee_amount(u64::MAX, 10_000).unwrap(), u64::MAX);
        assert_eq!(fee_amount(1_000, 0).unwrap(), 0);
    }

    #[test]
    fn winner_boundaries() {
        // Deposits own entries [0,10), [10,15), [15,45).
        let cum = [10u64, 15, 45];
        assert_eq!(winning_index(cum, 45, 0).unwrap(), (0, 0));
        assert_eq!(winning_index(cum, 45, 9).unwrap(), (0, 9));
        assert_eq!(winning_index(cum, 45, 10).unwrap(), (1, 10));
        assert_eq!(winning_index(cum, 45, 14).unwrap(), (1, 14));
        assert_eq!(winning_index(cum, 45, 15).unwrap(), (2, 15));
        assert_eq!(winning_index(cum, 45, 44).unwrap(), (2, 44));
        assert_eq!(winning_index(cum, 45, 45).unwrap(), (0, 0));
        assert_eq!(
            winning_index(cum, 45, u64::MAX).unwrap(),
            (2, u64::MAX % 45)
        );
    }

    #[test]
    fn winner_rejects_inconsistent_state() {
        assert!(winning_index([10u64], 0, 5).is_err());
        assert!(winning_index([10u64], 20, 15).is_err());
        assert!(winning_index(Vec::<u64>::new(), 1, 0).is_err());
        let one = [10u64];
        assert!(winning_index_sorted(1, |i| one[i], 0, 5).is_err());
        assert!(winning_index_sorted(1, |i| one[i], 20, 15).is_err());
        assert!(winning_index_sorted(0, |_| 0, 1, 0).is_err());
    }

    #[test]
    fn binary_search_boundaries() {
        let cum = [10u64, 15, 45];
        let f = |i: usize| cum[i];
        for (r, want) in [(0, 0), (9, 0), (10, 1), (14, 1), (15, 2), (44, 2), (45, 0)] {
            assert_eq!(
                winning_index_sorted(3, f, 45, r).unwrap().0,
                want,
                "r = {r}"
            );
        }
        // A large round: deposit i owns entries [i*3, i*3+3).
        let n = 200_000usize;
        let big = |i: usize| (i as u64 + 1) * 3;
        let total = n as u64 * 3;
        assert_eq!(winning_index_sorted(n, big, total, 0).unwrap().0, 0);
        assert_eq!(
            winning_index_sorted(n, big, total, total - 1).unwrap().0,
            n - 1
        );
        assert_eq!(
            winning_index_sorted(n, big, total, 300_001).unwrap().0,
            100_000
        );
    }

    fn cumulative(entries: &[u64]) -> Vec<u64> {
        entries
            .iter()
            .scan(0u64, |acc, e| {
                *acc += e;
                Some(*acc)
            })
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1_000))]

        /// The plan's 1,000-value property: the chosen deposit's cumulative range contains the winning entry.
        #[test]
        fn winner_range_contains_winning_entry(
            entries in proptest::collection::vec(1u64..=1_000_000, 1..=100),
            random_value in any::<u64>(),
        ) {
            let cum = cumulative(&entries);
            let total = *cum.last().unwrap();
            let (i, winning_entry) = winning_index(cum.iter().copied(), total, random_value).unwrap();
            let lower = if i == 0 { 0 } else { cum[i - 1] };
            prop_assert_eq!(winning_entry, random_value % total);
            prop_assert!(lower <= winning_entry && winning_entry < cum[i]);
        }

        /// Every entry maps to the deposit that owns it, so odds are exactly proportional to entries.
        #[test]
        fn every_entry_maps_to_its_owner(entries in proptest::collection::vec(1u64..=20, 1..=20)) {
            let cum = cumulative(&entries);
            let total = *cum.last().unwrap();
            let mut wins = vec![0u64; entries.len()];
            for r in 0..total {
                let (i, _) = winning_index(cum.iter().copied(), total, r).unwrap();
                wins[i] += 1;
            }
            prop_assert_eq!(wins, entries);
        }

        /// Binary search picks exactly what the linear scan picks, for any running totals and random value.
        #[test]
        fn binary_search_matches_linear_scan(
            entries in proptest::collection::vec(1u64..=1_000_000, 1..=2_000),
            random_value in any::<u64>(),
        ) {
            let cum = cumulative(&entries);
            let total = *cum.last().unwrap();
            let linear = winning_index(cum.iter().copied(), total, random_value).unwrap();
            let sorted = winning_index_sorted(cum.len(), |i| cum[i], total, random_value).unwrap();
            prop_assert_eq!(linear, sorted);
        }

        /// Every entry maps to the same owner under both searches (small rounds, all entries enumerated).
        #[test]
        fn binary_search_every_entry(entries in proptest::collection::vec(1u64..=8, 1..=40)) {
            let cum = cumulative(&entries);
            let total = *cum.last().unwrap();
            for r in 0..total {
                prop_assert_eq!(
                    winning_index(cum.iter().copied(), total, r).unwrap(),
                    winning_index_sorted(cum.len(), |i| cum[i], total, r).unwrap()
                );
            }
        }

        #[test]
        fn fee_is_floor(amount in any::<u64>(), fee_bps in 0u16..=MAX_FEE_BPS) {
            let fee = fee_amount(amount, fee_bps).unwrap();
            let exact = u128::from(amount) * u128::from(fee_bps);
            prop_assert!(u128::from(fee) * 10_000 <= exact);
            prop_assert!(exact < (u128::from(fee) + 1) * 10_000);
            prop_assert!(fee <= amount);
        }

        #[test]
        fn entries_are_floor(value in any::<u64>(), lpe in 1u64..=1_000_000_000) {
            let e = entries_for(value, lpe).unwrap();
            prop_assert!(u128::from(e) * u128::from(lpe) <= u128::from(value));
            prop_assert!(u128::from(value) < (u128::from(e) + 1) * u128::from(lpe));
        }
    }
}
