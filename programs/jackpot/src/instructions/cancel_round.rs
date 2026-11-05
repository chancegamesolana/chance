use anchor_lang::prelude::*;

use crate::{constants::*, error::JackpotError, events::RoundCancelled, state::RoundV2};

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct CancelRound<'info> {
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, RoundV2>,
}

pub fn handle_cancel_round(ctx: Context<CancelRound>, round_id: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let mut round = ctx.accounts.round.load_mut()?;
    let reason = round.cancel_reason(now).ok_or(JackpotError::CannotCancel)?;
    round.status = STATUS_CANCELLED;
    emit!(RoundCancelled { round_id, reason });
    Ok(())
}
