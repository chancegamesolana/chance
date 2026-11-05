use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::{RoundSettled, RoundSettledV2},
    randomness,
    state::{legacy_round_mut, round_layout, split_round_v2, RoundLayout},
};

/// MagicBlock VRF callback. Account order and argument layout are fixed by the request built in
/// `randomness::request_data`: accounts `[vrf identity, round]`, data `randomness ++ round_id`. Requests made
/// before the RoundV2 upgrade name a legacy `Round`, so the round account accepts either layout.
#[derive(Accounts)]
#[instruction(randomness: [u8; 32], round_id: u64)]
pub struct SettleRound<'info> {
    /// Scoped VRF identity PDA. Only the VRF program can sign as it, and only when fulfilling a request
    /// this program made.
    #[account(address = randomness::callback_identity() @ JackpotError::InvalidVrfIdentity)]
    pub vrf_program_identity: Signer<'info>,
    /// CHECK: round `round_id` of either layout; owner and discriminator are checked by `round_layout`.
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: UncheckedAccount<'info>,
}

pub fn handle_settle_round(
    ctx: Context<SettleRound>,
    randomness: [u8; 32],
    round_id: u64,
) -> Result<()> {
    let random_value = randomness::random_value(&randomness);
    let round_info = ctx.accounts.round.to_account_info();
    let layout = round_layout(&round_info)?;
    let mut data = round_info.try_borrow_mut_data()?;
    match layout {
        RoundLayout::V2 => {
            let (round, records) = split_round_v2(&mut data);
            let s = round.settle(random_value, records)?;
            emit!(RoundSettledV2 {
                round_id,
                winner: s.winner,
                winner_index: s.winner_index,
                randomness,
                random_value,
                winning_entry: s.winning_entry,
                total_entries: round.total_entries,
                pot_sol: round.total_sol_lamports,
                fee_sol: round.fee_sol_lamports,
            });
        }
        RoundLayout::Legacy => {
            let round = legacy_round_mut(&mut data);
            let s = round.settle(random_value)?;
            emit!(RoundSettled {
                round_id,
                winner: s.winner,
                winner_index: round.winner_index,
                randomness,
                random_value,
                winning_entry: s.winning_entry,
                total_entries: round.total_entries,
                pot_value: round.total_value,
                pot_sol: round.total_sol_lamports,
                pot_usdc: round.total_usdc,
                fee_sol: round.fee_sol_lamports,
                fee_usdc: round.fee_usdc,
            });
        }
    }
    Ok(())
}
