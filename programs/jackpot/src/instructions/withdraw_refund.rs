use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::{Refunded, TicketClosed},
    state::{release_records, RoundV2, SolVault, Ticket},
};

/// Permissionless since `RoundV2`: anyone may send it (e.g. the keeper refunding a cancelled round with many
/// players), but the refund, the ticket's rent and the player's record rent only ever go to the ticket's player.
#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct WithdrawRefund<'info> {
    /// CHECK: the ticket's player (bound by the ticket's seeds and `has_one`); only receives lamports.
    #[account(mut)]
    pub depositor: UncheckedAccount<'info>,
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, RoundV2>,
    #[account(mut, seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()], bump)]
    pub sol_vault: Account<'info, SolVault>,
    #[account(
        mut,
        close = depositor,
        seeds = [TICKET_SEED, &round_id.to_le_bytes(), depositor.key().as_ref()],
        bump = ticket.bump,
        constraint = ticket.player == depositor.key() @ JackpotError::InvalidTicket,
        constraint = ticket.round_id == round_id @ JackpotError::InvalidTicket,
    )]
    pub ticket: Account<'info, Ticket>,
}

/// Refunds everything the ticket's player deposited in a cancelled round and closes their ticket.
pub fn handle_withdraw_refund(ctx: Context<WithdrawRefund>, round_id: u64) -> Result<()> {
    let depositor = ctx.accounts.depositor.key();
    let ticket = &ctx.accounts.ticket;
    let sol_amount = ticket.amount;
    require!(sol_amount > 0, JackpotError::NothingToRefund);
    {
        let mut round = ctx.accounts.round.load_mut()?;
        require!(
            round.status == STATUS_CANCELLED,
            JackpotError::RoundNotCancelled
        );
        round.refunded_lamports = round
            .refunded_lamports
            .checked_add(sol_amount)
            .ok_or(JackpotError::MathOverflow)?;
        round.tickets_closed = round
            .tickets_closed
            .checked_add(1)
            .ok_or(JackpotError::MathOverflow)?;
    }

    ctx.accounts.sol_vault.sub_lamports(sol_amount)?;
    ctx.accounts.depositor.add_lamports(sol_amount)?;

    let records_rent = release_records(
        &ctx.accounts.round.to_account_info(),
        &ctx.accounts.depositor.to_account_info(),
        ticket.deposits,
        ticket.record_rent,
        &Rent::get()?,
    )?;
    let rent_lamports = records_rent
        .checked_add(ctx.accounts.ticket.to_account_info().lamports())
        .ok_or(JackpotError::MathOverflow)?;

    emit!(Refunded {
        round_id,
        depositor,
        sol_amount,
        usdc_amount: 0,
    });
    emit!(TicketClosed {
        round_id,
        player: depositor,
        rent_lamports,
    });
    Ok(())
}
