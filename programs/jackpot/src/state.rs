use anchor_lang::{prelude::*, Discriminator};

use crate::{constants::*, error::JackpotError, math, randomness::DEFAULT_ORACLE_QUEUE};

/// Global game settings. PDA `["config"]`.
///
/// Round-shaping fields (`fee_bps`, `lamports_per_entry`, durations, `min_deposit_lamports`, `max_pot_lamports`)
/// are snapshotted into each round at `open_round`, so config changes only affect future rounds.
///
/// The account's lamports are its rent plus `pending_fee_lamports` (fees the treasury could not take yet).
#[account]
#[derive(InitSpace)]
pub struct GameConfig {
    pub admin: Pubkey,
    /// Receives fees. If crediting a fee would leave it below the rent-exempt minimum (an empty treasury and a
    /// fee under ~0.0009 SOL), `withdraw_fees` parks the fee in this config account (`pending_fee_lamports`)
    /// and forwards it with a later fee once the treasury can take it.
    pub treasury: Pubkey,
    /// Fixed at initialization.
    /// Reserved for a future USDC phase; unused while SOL is the only asset.
    pub usdc_mint: Pubkey,
    /// Reserved for a future USDC phase (USDC-to-SOL conversion); unused.
    pub pyth_sol_usd_feed: Pubkey,
    /// MagicBlock VRF oracle queue that `close_round` sends randomness requests to. Must equal
    /// `randomness::DEFAULT_ORACLE_QUEUE`; changing the queue takes a program upgrade, like
    /// any other change to the hand-written VRF request.
    pub vrf_oracle_queue: Pubkey,
    pub fee_bps: u16,
    /// Lamports per entry; odds are proportional to SOL deposited.
    pub lamports_per_entry: u64,
    pub round_duration_secs: u32,
    /// Deposits are rejected in the last N seconds before cutoff.
    pub deposit_lock_secs: u32,
    pub reveal_timeout_secs: u32,
    /// Reserved: rounds have no deposit cap since `RoundV2`. Only legacy rounds snapshotted it.
    pub max_deposits: u16,
    /// Reserved: rounds have no participant cap since `RoundV2`. Only legacy rounds snapshotted it.
    pub max_participants: u16,
    pub min_deposit_lamports: u64,
    /// Reserved for a future USDC phase; unused.
    pub max_price_age_secs: u32,
    /// Per-round pot cap in lamports; 0 disables it.
    pub max_pot_lamports: u64,
    pub paused: bool,
    /// Id of the most recently opened round; 0 before the first round.
    pub current_round_id: u64,
    pub bump: u8,
    // The next two fields were carved out of the former `_reserved: [u8; 64]` (same bytes, same
    // size; existing accounts hold zeros there, i.e. no pending admin and no pending fees).
    /// Admin proposed by `update_config { admin }`; becomes `admin` when it signs `accept_admin`. Default = none.
    pub pending_admin: Pubkey,
    /// Fees held in this account because the treasury could not take them yet (see `treasury`).
    pub pending_fee_lamports: u64,
    pub _reserved: [u8; 24],
}

impl GameConfig {
    /// Checks every tunable. Called after `initialize_config` and `update_config` write their changes.
    pub fn validate(&self) -> Result<()> {
        require!(self.fee_bps <= MAX_FEE_BPS, JackpotError::FeeTooHigh);
        require_keys_neq!(self.admin, Pubkey::default(), JackpotError::InvalidConfig);
        require_keys_neq!(
            self.treasury,
            Pubkey::default(),
            JackpotError::InvalidConfig
        );
        require_keys_eq!(
            self.vrf_oracle_queue,
            DEFAULT_ORACLE_QUEUE,
            JackpotError::InvalidOracleQueue
        );
        require!(self.lamports_per_entry > 0, JackpotError::InvalidConfig);
        // Guarantees every accepted deposit is worth at least one entry.
        require!(
            self.min_deposit_lamports >= self.lamports_per_entry,
            JackpotError::InvalidConfig
        );
        require!(
            self.round_duration_secs > 0 && self.round_duration_secs <= MAX_ROUND_DURATION_SECS,
            JackpotError::InvalidConfig
        );
        // The lock may cover at most half the round, so there is always a real deposit window.
        require!(
            self.deposit_lock_secs <= self.round_duration_secs / 2,
            JackpotError::InvalidConfig
        );
        require!(
            (MIN_REVEAL_TIMEOUT_SECS..=MAX_REVEAL_TIMEOUT_SECS).contains(&self.reveal_timeout_secs),
            JackpotError::InvalidConfig
        );
        require!(self.max_price_age_secs > 0, JackpotError::InvalidConfig);
        require!(
            self.max_pot_lamports == 0 || self.max_pot_lamports >= self.min_deposit_lamports,
            JackpotError::InvalidConfig
        );
        Ok(())
    }
}

/// Holds a round's SOL. PDA `["sol_vault", round_id]`, program-owned so the program can debit it directly.
/// Its balance above the rent-exempt minimum is exactly the round's undistributed SOL.
#[account]
#[derive(InitSpace)]
pub struct SolVault {}

// ------------------------------------------------------------------------------------------------------------
// RoundV2: rounds opened since the RoundV2 upgrade. No deposit or participant cap.
// ------------------------------------------------------------------------------------------------------------

/// One round. PDA `["round", round_id]`, the same seeds as legacy rounds (round ids never repeat, and the
/// discriminator tells the two layouts apart).
///
/// The account is this fixed zero-copy header followed by a growable tail of `DEPOSIT_RECORD_SIZE`-byte deposit
/// records (`depositor | amount u64 | cumulative_entries u64`, little-endian), one per deposit in order.
/// `deposit_sol` grows the account by one record, paid by the depositor. Once the round is Drawn or Cancelled
/// the records are no longer read, and closing each player's `Ticket` shrinks the account by that player's
/// records and returns their rent; when every ticket is closed only the header is left.
#[account(zero_copy)]
pub struct RoundV2 {
    pub id: u64,
    pub opened_at: i64,
    pub cutoff_at: i64,
    /// Slot at which `close_round` ran.
    pub closed_slot: u64,
    pub closed_at: i64,
    /// VRF oracle queue the randomness request was sent to at close.
    pub vrf_queue: Pubkey,
    pub random_value: u64,
    pub winner: Pubkey,
    /// Paid rent for the round header and vault accounts.
    pub opener: Pubkey,
    pub total_entries: u64,
    /// Sum of deposit values in lamports (equals `total_sol_lamports` while SOL is the only asset).
    pub total_value: u64,
    pub total_sol_lamports: u64,
    pub fee_sol_lamports: u64,
    /// Lamports refunded so far (cancelled rounds).
    pub refunded_lamports: u64,
    // Rules snapshotted from `GameConfig` at open.
    pub lamports_per_entry: u64,
    pub min_deposit_lamports: u64,
    pub max_pot_lamports: u64,
    pub deposit_lock_secs: u32,
    pub reveal_timeout_secs: u32,
    /// Number of deposits ever made (records written).
    pub deposits_len: u32,
    /// Number of distinct depositors (= tickets created).
    pub participants_len: u32,
    pub winner_index: u32,
    /// Tickets closed so far (by `withdraw_refund` or `close_tickets`).
    pub tickets_closed: u32,
    pub fee_bps: u16,
    /// One of `STATUS_*`.
    pub status: u8,
    pub prize_claimed: u8,
    pub fees_claimed: u8,
    pub bump: u8,
    pub sol_vault_bump: u8,
    pub _padding: [u8; 1],
    pub _reserved: [u8; 64],
}

/// Bytes of one deposit record in a `RoundV2` account's tail.
pub const DEPOSIT_RECORD_SIZE: usize = 48;

/// Offset of the first deposit record in a `RoundV2` account.
pub const ROUND_V2_HEADER_LEN: usize = 8 + RoundV2::SIZE;

/// Largest number of deposits one round can hold (the 10 MiB account size limit).
pub const MAX_ROUND_V2_DEPOSITS: usize =
    (MAX_ACCOUNT_DATA_LEN - ROUND_V2_HEADER_LEN) / DEPOSIT_RECORD_SIZE;

/// The `index`-th record's depositor.
pub fn record_depositor(records: &[u8], index: usize) -> Result<Pubkey> {
    let o = index
        .checked_mul(DEPOSIT_RECORD_SIZE)
        .ok_or(JackpotError::MathOverflow)?;
    let bytes: [u8; 32] = records
        .get(o..o + 32)
        .and_then(|b| b.try_into().ok())
        .ok_or(JackpotError::InconsistentRoundState)?;
    Ok(Pubkey::new_from_array(bytes))
}

/// The `index`-th record's running entry total, or `None` if the record isn't there.
pub fn record_cumulative(records: &[u8], index: usize) -> Option<u64> {
    let o = index.checked_mul(DEPOSIT_RECORD_SIZE)?.checked_add(40)?;
    let bytes: [u8; 8] = records.get(o..o + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

/// Writes the `index`-th record.
pub fn write_record(
    records: &mut [u8],
    index: usize,
    depositor: &Pubkey,
    amount: u64,
    cumulative_entries: u64,
) -> Result<()> {
    let o = index
        .checked_mul(DEPOSIT_RECORD_SIZE)
        .ok_or(JackpotError::MathOverflow)?;
    let rec = records
        .get_mut(o..o + DEPOSIT_RECORD_SIZE)
        .ok_or(JackpotError::InconsistentRoundState)?;
    rec[..32].copy_from_slice(depositor.as_ref());
    rec[32..40].copy_from_slice(&amount.to_le_bytes());
    rec[40..48].copy_from_slice(&cumulative_entries.to_le_bytes());
    Ok(())
}

/// The cancel rule shared by both round layouts (see `RoundV2::cancel_reason`). The SDK mirrors it in
/// `cancelReason` (sdk/src/state.ts); keep the two in step.
///
/// Deadlines: an open round may be cancelled for lack of players at `cutoff_at`, and for lack of a close
/// strictly after `cutoff_at + reveal_timeout_secs`. A drawing round may be cancelled strictly after
/// `max(cutoff_at, closed_at) + reveal_timeout_secs`, so the oracle always gets the full timeout from the
/// close, however late the close landed (and early pot-cap closes keep the window from the cutoff).
pub fn cancel_reason(
    status: u8,
    now: i64,
    cutoff_at: i64,
    closed_at: i64,
    reveal_timeout_secs: u32,
    too_few_participants: bool,
) -> Option<u8> {
    let timeout = i64::from(reveal_timeout_secs);
    match status {
        STATUS_OPEN => {
            if now >= cutoff_at && too_few_participants {
                return Some(CANCEL_NOT_ENOUGH_PARTICIPANTS);
            }
            let deadline = cutoff_at.checked_add(timeout)?;
            (now > deadline).then_some(CANCEL_CLOSE_TIMEOUT)
        }
        STATUS_DRAWING => {
            let deadline = cutoff_at.max(closed_at).checked_add(timeout)?;
            (now > deadline).then_some(CANCEL_REVEAL_TIMEOUT)
        }
        _ => None,
    }
}

/// What `RoundV2::record_deposit` accepted.
pub struct DepositOutcome {
    pub index: u32,
    pub entries: u64,
    pub cumulative_entries: u64,
    /// Join order of the depositor (0-based); for a repeat depositor the caller keeps their ticket's index.
    pub participant_index: u32,
}

/// Outcome of a settle.
pub struct Settlement {
    pub winner: Pubkey,
    pub winner_index: u32,
    pub winning_entry: u64,
}

impl RoundV2 {
    pub const SIZE: usize = core::mem::size_of::<RoundV2>();

    /// Why the round may be cancelled now, if it may: an open round that reached its cutoff with fewer than
    /// two participants; an open round that nobody managed to close within the reveal timeout of its cutoff
    /// (the VRF request CPI may be failing for good, and the deposits must not stay locked); or a drawing
    /// round whose VRF callback hasn't arrived within the reveal timeout of its close.
    pub fn cancel_reason(&self, now: i64) -> Option<u8> {
        cancel_reason(
            self.status,
            now,
            self.cutoff_at,
            self.closed_at,
            self.reveal_timeout_secs,
            self.participants_len < 2,
        )
    }

    /// True once the round owes no SOL to anyone: Drawn with prize and fees paid out, or Cancelled with every
    /// player refunded (each refund closes that player's ticket).
    pub fn is_finished(&self) -> bool {
        match self.status {
            STATUS_DRAWN => self.prize_claimed != 0 && self.fees_claimed != 0,
            STATUS_CANCELLED => self.tickets_closed >= self.participants_len,
            _ => false,
        }
    }

    /// Every ticket has been closed, so every depositor has their record and ticket rent back.
    pub fn tickets_settled(&self) -> bool {
        self.tickets_closed >= self.participants_len
    }

    /// A round may close at its cutoff, or early once the pot cap leaves no room for another minimum deposit.
    pub fn ready_to_close(&self, now: i64) -> bool {
        now >= self.cutoff_at
            || (self.max_pot_lamports != 0
                && self.max_pot_lamports.saturating_sub(self.total_value)
                    < self.min_deposit_lamports)
    }

    /// Validates a SOL deposit against the round's rules and updates the totals and counters. The caller
    /// writes the record, updates the ticket and moves the funds.
    pub fn record_deposit(
        &mut self,
        amount: u64,
        now: i64,
        new_participant: bool,
    ) -> Result<DepositOutcome> {
        require!(self.status == STATUS_OPEN, JackpotError::RoundNotOpen);
        let deadline = self
            .cutoff_at
            .checked_sub(i64::from(self.deposit_lock_secs))
            .ok_or(JackpotError::MathOverflow)?;
        require!(now < deadline, JackpotError::DepositWindowClosed);
        require!(
            amount >= self.min_deposit_lamports,
            JackpotError::BelowMinDeposit
        );
        let entries = math::entries_for(amount, self.lamports_per_entry)?;
        require!(entries > 0, JackpotError::ZeroEntries);
        let total_value = self
            .total_value
            .checked_add(amount)
            .ok_or(JackpotError::MathOverflow)?;
        require!(
            self.max_pot_lamports == 0 || total_value <= self.max_pot_lamports,
            JackpotError::PotCapExceeded
        );
        let cumulative_entries = self
            .total_entries
            .checked_add(entries)
            .ok_or(JackpotError::MathOverflow)?;
        let index = self.deposits_len;
        require!(
            usize::try_from(index).is_ok_and(|i| i < MAX_ROUND_V2_DEPOSITS),
            JackpotError::RoundFull
        );
        let participant_index = self.participants_len;
        self.deposits_len = index.checked_add(1).ok_or(JackpotError::MathOverflow)?;
        if new_participant {
            self.participants_len = participant_index
                .checked_add(1)
                .ok_or(JackpotError::MathOverflow)?;
        }
        self.total_entries = cumulative_entries;
        self.total_value = total_value;
        self.total_sol_lamports = self
            .total_sol_lamports
            .checked_add(amount)
            .ok_or(JackpotError::MathOverflow)?;
        Ok(DepositOutcome {
            index,
            entries,
            cumulative_entries,
            participant_index,
        })
    }

    /// Picks the winner from `random_value` by binary search over the records' running entry totals (same
    /// rule as the linear scan: the first deposit whose cumulative entries exceed `random_value %
    /// total_entries`), computes the fee, and marks the round Drawn.
    pub fn settle(&mut self, random_value: u64, records: &[u8]) -> Result<Settlement> {
        require!(self.status == STATUS_DRAWING, JackpotError::RoundNotDrawing);
        let len = usize::try_from(self.deposits_len).map_err(|_| JackpotError::MathOverflow)?;
        let needed = len
            .checked_mul(DEPOSIT_RECORD_SIZE)
            .ok_or(JackpotError::MathOverflow)?;
        require!(
            records.len() >= needed,
            JackpotError::InconsistentRoundState
        );
        let (index, winning_entry) = math::winning_index_sorted(
            len,
            |i| record_cumulative(records, i).unwrap_or(0),
            self.total_entries,
            random_value,
        )?;
        let winner = record_depositor(records, index)?;
        self.winner = winner;
        self.winner_index = u32::try_from(index).map_err(|_| JackpotError::MathOverflow)?;
        self.random_value = random_value;
        self.fee_sol_lamports = math::fee_amount(self.total_sol_lamports, self.fee_bps)?;
        self.status = STATUS_DRAWN;
        Ok(Settlement {
            winner,
            winner_index: self.winner_index,
            winning_entry,
        })
    }
}

/// One player's stake in one `RoundV2`. PDA `["ticket", round_id, player]`, created by the player's first
/// deposit (they pay its rent). Makes "is this a new participant?" and refunds O(1) however many deposits a
/// round holds, and remembers the record rent the player paid so it can be returned.
///
/// Closed by `withdraw_refund` (cancelled rounds) or `close_tickets` (drawn rounds, permissionless); either way
/// its rent and the player's record rent go back to the player.
#[account]
#[derive(InitSpace)]
pub struct Ticket {
    pub round_id: u64,
    pub player: Pubkey,
    /// Lamports deposited in total.
    pub amount: u64,
    pub entries: u64,
    /// Lamports the player paid to grow the round account by their deposit records.
    pub record_rent: u64,
    /// Number of deposit records the player added.
    pub deposits: u32,
    /// Join order within the round (0-based).
    pub index: u32,
    pub bump: u8,
}

// ------------------------------------------------------------------------------------------------------------
// Legacy round layout (rounds opened before the RoundV2 upgrade). Kept so in-flight legacy rounds can
// still close, settle, pay out, refund and close. New rounds never use it.
// ------------------------------------------------------------------------------------------------------------

/// Legacy round with a fixed 100-slot deposit array. PDA `["round", round_id]`.
#[account(zero_copy)]
pub struct Round {
    pub id: u64,
    pub opened_at: i64,
    pub cutoff_at: i64,
    /// Slot at which `close_round` ran.
    pub closed_slot: u64,
    pub closed_at: i64,
    /// VRF oracle queue the randomness request was sent to at close.
    pub vrf_queue: Pubkey,
    pub random_value: u64,
    pub winner: Pubkey,
    /// Paid rent for the round and vault accounts.
    pub opener: Pubkey,
    pub total_entries: u64,
    /// Sum of deposit values in lamports (equals `total_sol_lamports` while SOL is the only asset).
    pub total_value: u64,
    pub total_sol_lamports: u64,
    pub total_usdc: u64,
    pub fee_sol_lamports: u64,
    pub fee_usdc: u64,
    // Rules snapshotted from `GameConfig` at open.
    pub lamports_per_entry: u64,
    pub min_deposit_lamports: u64,
    pub max_pot_lamports: u64,
    pub deposit_lock_secs: u32,
    pub reveal_timeout_secs: u32,
    pub fee_bps: u16,
    pub max_deposits: u16,
    pub max_participants: u16,
    pub deposits_len: u16,
    pub participants_len: u16,
    pub winner_index: u16,
    /// One of `STATUS_*`.
    pub status: u8,
    pub prize_claimed: u8,
    pub fees_claimed: u8,
    pub bump: u8,
    pub sol_vault_bump: u8,
    pub _padding: [u8; 7],
    pub _reserved: [u8; 64],
    pub deposits: [Deposit; LEGACY_MAX_DEPOSITS],
}

#[zero_copy]
pub struct Deposit {
    pub depositor: Pubkey,
    /// Lamports or USDC base units, depending on `asset`.
    pub amount: u64,
    /// Value in lamports; entries are derived from it. Equal to `amount` for SOL deposits.
    pub value: u64,
    /// Running total of entries including this deposit.
    pub cumulative_entries: u64,
    /// One of `ASSET_*`.
    pub asset: u8,
    /// Nonzero once refunded from a cancelled round.
    pub refunded: u8,
    pub _padding: [u8; 6],
}

impl Round {
    pub const SIZE: usize = core::mem::size_of::<Round>();

    /// The populated part of the deposit array.
    pub fn deposits(&self) -> &[Deposit] {
        let len = usize::from(self.deposits_len).min(LEGACY_MAX_DEPOSITS);
        &self.deposits[..len]
    }

    pub fn deposits_mut(&mut self) -> &mut [Deposit] {
        let len = usize::from(self.deposits_len).min(LEGACY_MAX_DEPOSITS);
        &mut self.deposits[..len]
    }

    /// Same rules as `RoundV2::cancel_reason`.
    pub fn cancel_reason(&self, now: i64) -> Option<u8> {
        cancel_reason(
            self.status,
            now,
            self.cutoff_at,
            self.closed_at,
            self.reveal_timeout_secs,
            self.participants_len < 2,
        )
    }

    /// True once the round owes nothing to anyone: Drawn with prize and fees paid out, or Cancelled with
    /// every deposit refunded. Only then may its accounts be closed.
    pub fn is_finished(&self) -> bool {
        match self.status {
            STATUS_DRAWN => self.prize_claimed != 0 && self.fees_claimed != 0,
            STATUS_CANCELLED => self.deposits().iter().all(|d| d.refunded != 0),
            _ => false,
        }
    }

    /// A legacy round may close at its cutoff, or early once one of its snapshotted caps is hit.
    pub fn ready_to_close(&self, now: i64) -> bool {
        now >= self.cutoff_at
            || self.deposits_len >= self.max_deposits
            || self.participants_len >= self.max_participants
    }

    /// Picks the winner from `random_value`, computes fees, and marks the round Drawn.
    pub fn settle(&mut self, random_value: u64) -> Result<Settlement> {
        require!(self.status == STATUS_DRAWING, JackpotError::RoundNotDrawing);
        let (index, winning_entry) = math::winning_index(
            self.deposits().iter().map(|d| d.cumulative_entries),
            self.total_entries,
            random_value,
        )?;
        let winner = self.deposits[index].depositor;
        self.winner = winner;
        self.winner_index = u16::try_from(index).map_err(|_| JackpotError::MathOverflow)?;
        self.random_value = random_value;
        self.fee_sol_lamports = math::fee_amount(self.total_sol_lamports, self.fee_bps)?;
        self.fee_usdc = math::fee_amount(self.total_usdc, self.fee_bps)?;
        self.status = STATUS_DRAWN;
        Ok(Settlement {
            winner,
            winner_index: u32::from(self.winner_index),
            winning_entry,
        })
    }
}

// ------------------------------------------------------------------------------------------------------------
// Reading a round account of either layout (instructions whose round account is fixed before the upgrade:
// the VRF callback, and close/open which must keep working for in-flight legacy rounds).
// ------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoundLayout {
    Legacy,
    V2,
}

/// Which layout a round account holds. Errors like Anchor's own loaders: a closed (system-owned) PDA is
/// `AccountOwnedByWrongProgram`, anything else unrecognised is `AccountDiscriminatorMismatch`.
pub fn round_layout(info: &AccountInfo) -> Result<RoundLayout> {
    if info.owner != &crate::ID {
        return Err(Error::from(ErrorCode::AccountOwnedByWrongProgram)
            .with_pubkeys((*info.owner, crate::ID)));
    }
    let data = info.try_borrow_data()?;
    let disc = data
        .get(..8)
        .ok_or(ErrorCode::AccountDiscriminatorNotFound)?;
    if disc == RoundV2::DISCRIMINATOR && data.len() >= ROUND_V2_HEADER_LEN {
        Ok(RoundLayout::V2)
    } else if disc == Round::DISCRIMINATOR && data.len() >= 8 + Round::SIZE {
        Ok(RoundLayout::Legacy)
    } else {
        err!(ErrorCode::AccountDiscriminatorMismatch)
    }
}

/// Splits a `RoundV2` account's data into its header and its record tail. The caller checked the layout.
pub fn split_round_v2(data: &mut [u8]) -> (&mut RoundV2, &mut [u8]) {
    let (head, tail) = data.split_at_mut(ROUND_V2_HEADER_LEN);
    (bytemuck::from_bytes_mut(&mut head[8..]), tail)
}

/// A legacy round's header and deposit array. The caller checked the layout.
pub fn legacy_round_mut(data: &mut [u8]) -> &mut Round {
    bytemuck::from_bytes_mut(&mut data[8..8 + Round::SIZE])
}

/// The status byte of a round account of either layout.
pub fn round_status(info: &AccountInfo) -> Result<u8> {
    let layout = round_layout(info)?;
    let data = info.try_borrow_data()?;
    let offset = match layout {
        RoundLayout::V2 => 8 + core::mem::offset_of!(RoundV2, status),
        RoundLayout::Legacy => 8 + core::mem::offset_of!(Round, status),
    };
    Ok(data[offset])
}

/// Shrinks a finished `RoundV2` account by `records` deposit records and pays up to `paid` lamports of the
/// rent they held to `to`, never taking the account below the rent-exempt minimum for its new size. Returns
/// the lamports paid. The records of a Drawn or Cancelled round are never read again.
pub fn release_records<'info>(
    round: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    records: u32,
    paid: u64,
    rent: &Rent,
) -> Result<u64> {
    let cut = usize::try_from(records)
        .ok()
        .and_then(|n| n.checked_mul(DEPOSIT_RECORD_SIZE))
        .ok_or(JackpotError::MathOverflow)?;
    let new_len = round
        .data_len()
        .checked_sub(cut)
        .filter(|len| *len >= ROUND_V2_HEADER_LEN)
        .ok_or(JackpotError::InconsistentRoundState)?;
    round.resize(new_len)?;
    let spare = round
        .lamports()
        .saturating_sub(rent.minimum_balance(new_len));
    let amount = paid.min(spare);
    round.sub_lamports(amount)?;
    to.add_lamports(amount)?;
    Ok(amount)
}

// Layout is part of the on-chain contract; changing it needs a migration.
const _: () = assert!(core::mem::size_of::<Deposit>() == 64);
const _: () = assert!(Round::SIZE == 312 + 64 * LEGACY_MAX_DEPOSITS);
const _: () = assert!(RoundV2::SIZE == 304);
const _: () = assert!(ROUND_V2_HEADER_LEN % 8 == 0);
// `GameConfig` as first deployed: 5 pubkeys, the tunables, flags, round id, bump, 64 reserved bytes.
// `pending_admin` and `pending_fee_lamports` live inside those 64 bytes, so the size never changed.
const _: () = assert!(GameConfig::INIT_SPACE == 280);

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    fn config() -> GameConfig {
        GameConfig {
            admin: Pubkey::new_unique(),
            treasury: Pubkey::new_unique(),
            usdc_mint: Pubkey::new_unique(),
            pyth_sol_usd_feed: Pubkey::new_unique(),
            vrf_oracle_queue: DEFAULT_ORACLE_QUEUE,
            fee_bps: 350,
            lamports_per_entry: 1,
            round_duration_secs: 60,
            deposit_lock_secs: 0,
            reveal_timeout_secs: 600,
            max_deposits: 0,
            max_participants: 0,
            min_deposit_lamports: 1_000_000,
            max_price_age_secs: 60,
            max_pot_lamports: 0,
            paused: false,
            current_round_id: 0,
            bump: 255,
            pending_admin: Pubkey::default(),
            pending_fee_lamports: 0,
            _reserved: [0; 24],
        }
    }

    /// The devnet config (20 s rounds in the smoke script, 600 s reveal timeout, 0.001 SOL min deposit) and
    /// the current 60 s rounds both pass.
    #[test]
    fn validate_accepts_the_devnet_configs() {
        assert!(config().validate().is_ok());
        let mut c = config();
        c.round_duration_secs = 20;
        c.deposit_lock_secs = 10;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_bounds_the_tunables() {
        type Case = (&'static str, fn(&mut GameConfig));
        let cases: [Case; 9] = [
            ("tiny reveal timeout", |c| c.reveal_timeout_secs = 59),
            ("huge reveal timeout", |c| {
                c.reveal_timeout_secs = MAX_REVEAL_TIMEOUT_SECS + 1
            }),
            ("zero round", |c| c.round_duration_secs = 0),
            ("huge round", |c| {
                c.round_duration_secs = MAX_ROUND_DURATION_SECS + 1
            }),
            ("lock over half the round", |c| c.deposit_lock_secs = 31),
            ("foreign queue", |c| {
                c.vrf_oracle_queue = Pubkey::new_unique()
            }),
            ("default queue", |c| c.vrf_oracle_queue = Pubkey::default()),
            ("fee over cap", |c| c.fee_bps = MAX_FEE_BPS + 1),
            ("entry worth more than min deposit", |c| {
                c.lamports_per_entry = c.min_deposit_lamports + 1
            }),
        ];
        for (name, mutate) in cases {
            let mut c = config();
            mutate(&mut c);
            assert!(c.validate().is_err(), "{name} should be rejected");
        }
        let mut edge = config();
        edge.reveal_timeout_secs = MIN_REVEAL_TIMEOUT_SECS;
        edge.round_duration_secs = MAX_ROUND_DURATION_SECS;
        edge.deposit_lock_secs = MAX_ROUND_DURATION_SECS / 2;
        assert!(edge.validate().is_ok());
    }

    #[test]
    fn open_round_cancels_for_lack_of_players_at_cutoff_only() {
        let few = |now| cancel_reason(STATUS_OPEN, now, 1_000, 0, 600, true);
        assert_eq!(few(999), None);
        assert_eq!(few(1_000), Some(CANCEL_NOT_ENOUGH_PARTICIPANTS));
        assert_eq!(few(5_000), Some(CANCEL_NOT_ENOUGH_PARTICIPANTS));
    }

    #[test]
    fn open_round_with_players_cancels_only_after_the_close_timeout() {
        let full = |now| cancel_reason(STATUS_OPEN, now, 1_000, 0, 600, false);
        assert_eq!(full(999), None);
        assert_eq!(full(1_000), None);
        assert_eq!(full(1_600), None); // exactly cutoff + timeout: a close still wins
        assert_eq!(full(1_601), Some(CANCEL_CLOSE_TIMEOUT));
        assert_eq!(
            cancel_reason(STATUS_OPEN, i64::MAX, i64::MAX, 0, 600, false),
            None
        ); // overflow: never
    }

    #[test]
    fn drawing_round_timeout_runs_from_the_close() {
        // Closed late (keeper outage): the oracle still gets the full timeout from the close.
        let late = |now| cancel_reason(STATUS_DRAWING, now, 1_000, 1_900, 600, false);
        assert_eq!(late(1_601), None);
        assert_eq!(late(2_500), None);
        assert_eq!(late(2_501), Some(CANCEL_REVEAL_TIMEOUT));
        // Closed early (pot cap): the window still runs from the cutoff.
        let early = |now| cancel_reason(STATUS_DRAWING, now, 1_000, 500, 600, false);
        assert_eq!(early(1_101), None);
        assert_eq!(early(1_600), None);
        assert_eq!(early(1_601), Some(CANCEL_REVEAL_TIMEOUT));
        // Participant count is irrelevant once drawing.
        assert_eq!(
            cancel_reason(STATUS_DRAWING, 2_501, 1_000, 1_900, 600, true),
            Some(CANCEL_REVEAL_TIMEOUT)
        );
    }

    #[test]
    fn finished_rounds_never_cancel() {
        for status in [STATUS_DRAWN, STATUS_CANCELLED, 200] {
            assert_eq!(cancel_reason(status, i64::MAX, 0, 0, 60, true), None);
        }
    }

    #[test]
    fn both_layouts_apply_the_shared_rule() {
        let mut v2 = RoundV2::zeroed();
        v2.status = STATUS_OPEN;
        v2.cutoff_at = 1_000;
        v2.reveal_timeout_secs = 600;
        v2.participants_len = 2;
        assert_eq!(v2.cancel_reason(1_600), None);
        assert_eq!(v2.cancel_reason(1_601), Some(CANCEL_CLOSE_TIMEOUT));
        v2.status = STATUS_DRAWING;
        v2.closed_at = 1_601;
        assert_eq!(v2.cancel_reason(2_201), None);
        assert_eq!(v2.cancel_reason(2_202), Some(CANCEL_REVEAL_TIMEOUT));

        let mut legacy = Round::zeroed();
        legacy.status = STATUS_OPEN;
        legacy.cutoff_at = 1_000;
        legacy.reveal_timeout_secs = 600;
        legacy.participants_len = 2;
        assert_eq!(legacy.cancel_reason(1_600), None);
        assert_eq!(legacy.cancel_reason(1_601), Some(CANCEL_CLOSE_TIMEOUT));
        legacy.status = STATUS_DRAWING;
        legacy.closed_at = 1_601;
        assert_eq!(legacy.cancel_reason(2_201), None);
        assert_eq!(legacy.cancel_reason(2_202), Some(CANCEL_REVEAL_TIMEOUT));
    }
}
