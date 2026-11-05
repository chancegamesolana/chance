use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::RoundAccountsClosed,
    state::{RoundV2, SolVault},
};

/// Permissionless: anyone may close a finished round, but the rent always returns to its opener. Every ticket
/// must be closed first, so every depositor has had their record rent back.
#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct CloseRoundAccounts<'info> {
    /// CHECK: must equal `round.opener` (checked in the handler); receives the rent of both accounts.
    #[account(mut)]
    pub opener: UncheckedAccount<'info>,
    #[account(
        mut,
        close = opener,
        seeds = [ROUND_SEED, &round_id.to_le_bytes()],
        bump
    )]
    pub round: AccountLoader<'info, RoundV2>,
    #[account(
        mut,
        close = opener,
        seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()],
        bump
    )]
    pub sol_vault: Account<'info, SolVault>,
}

pub fn handle_close_round_accounts(ctx: Context<CloseRoundAccounts>, round_id: u64) -> Result<()> {
    {
        let round = ctx.accounts.round.load()?;
        require_keys_eq!(
            ctx.accounts.opener.key(),
            round.opener,
            JackpotError::InvalidRentReceiver
        );
        require!(round.is_finished(), JackpotError::RoundNotSettled);
        require!(round.tickets_settled(), JackpotError::TicketsOutstanding);
    }
    emit!(RoundAccountsClosed {
        round_id,
        rent_receiver: ctx.accounts.opener.key(),
    });
    Ok(())
}
