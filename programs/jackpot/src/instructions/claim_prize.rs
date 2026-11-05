use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::PrizeClaimed,
    state::{RoundV2, SolVault},
};

/// Permissionless: anyone may trigger the payout, but it only ever goes to `round.winner`.
#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct ClaimPrize<'info> {
    /// CHECK: must equal `round.winner` (checked in the handler); only receives lamports.
    #[account(mut)]
    pub winner: UncheckedAccount<'info>,
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, RoundV2>,
    #[account(mut, seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()], bump)]
    pub sol_vault: Account<'info, SolVault>,
}

pub fn handle_claim_prize(ctx: Context<ClaimPrize>, round_id: u64) -> Result<()> {
    let winner = ctx.accounts.winner.key();
    let sol_amount = {
        let mut round = ctx.accounts.round.load_mut()?;
        require!(round.status == STATUS_DRAWN, JackpotError::RoundNotDrawn);
        require_keys_eq!(winner, round.winner, JackpotError::NotWinner);
        require!(round.prize_claimed == 0, JackpotError::PrizeAlreadyClaimed);
        round.prize_claimed = 1;
        round
            .total_sol_lamports
            .checked_sub(round.fee_sol_lamports)
            .ok_or(JackpotError::MathOverflow)?
    };

    ctx.accounts.sol_vault.sub_lamports(sol_amount)?;
    ctx.accounts.winner.add_lamports(sol_amount)?;

    emit!(PrizeClaimed {
        round_id,
        winner,
        sol_amount,
        usdc_amount: 0,
    });
    Ok(())
}
