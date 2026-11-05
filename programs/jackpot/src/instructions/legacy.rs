//! Finishing legacy rounds (the fixed 100-deposit `Round` layout used before the RoundV2 upgrade).
//!
//! A legacy round that was in flight during the upgrade still closes and settles through `close_round` /
//! `settle_round`, which accept both layouts. Everything after that is this one permissionless crank, which
//! applies the legacy rules unchanged: cancel when the round may be cancelled, pay the prize to the winner and the
//! fees to the treasury, refund the depositors passed as remaining accounts, and close the round and its vault
//! (rent to the opener) once it owes nothing. Funds only ever go to the winner, the treasury, the depositor
//! whose deposits are refunded, or the opener. No legacy round can be opened, and legacy rounds take no deposits.
//! Remove this module once no legacy round account exists on any cluster.

use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::{FeesWithdrawn, PrizeClaimed, Refunded, RoundAccountsClosed, RoundCancelled},
    state::{GameConfig, Round, SolVault},
};

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct LegacyFinishRound<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = treasury @ JackpotError::InvalidTreasury
    )]
    pub config: Account<'info, GameConfig>,
    /// CHECK: must equal `config.treasury` (has_one above); only receives lamports.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: receives the prize only if it equals `round.winner` (checked in the handler).
    #[account(mut)]
    pub winner: UncheckedAccount<'info>,
    /// CHECK: must equal `round.opener` (checked in the handler); receives the rent when the round closes.
    #[account(mut)]
    pub opener: UncheckedAccount<'info>,
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, Round>,
    #[account(mut, seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()], bump)]
    pub sol_vault: Account<'info, SolVault>,
}

pub fn handle_legacy_finish_round<'info>(
    ctx: Context<'info, LegacyFinishRound<'info>>,
    round_id: u64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let winner = ctx.accounts.winner.key();
    let mut prize: Option<u64> = None;
    let mut fees: Option<u64> = None;
    let mut refunds: Vec<(usize, u64)> = Vec::new();
    let mut acted = false;
    let finished = {
        let mut round = ctx.accounts.round.load_mut()?;
        require_keys_eq!(
            ctx.accounts.opener.key(),
            round.opener,
            JackpotError::InvalidRentReceiver
        );
        require!(
            round.total_usdc == 0 && round.fee_usdc == 0,
            JackpotError::UnsupportedAsset
        );
        if let Some(reason) = round.cancel_reason(now) {
            round.status = STATUS_CANCELLED;
            acted = true;
            emit!(RoundCancelled { round_id, reason });
        }
        match round.status {
            STATUS_DRAWN => {
                if round.prize_claimed == 0 {
                    require_keys_eq!(winner, round.winner, JackpotError::NotWinner);
                    round.prize_claimed = 1;
                    prize = Some(
                        round
                            .total_sol_lamports
                            .checked_sub(round.fee_sol_lamports)
                            .ok_or(JackpotError::MathOverflow)?,
                    );
                    acted = true;
                }
                if round.fees_claimed == 0 {
                    round.fees_claimed = 1;
                    fees = Some(round.fee_sol_lamports);
                    acted = true;
                }
            }
            STATUS_CANCELLED => {
                for (i, depositor) in ctx.remaining_accounts.iter().enumerate() {
                    require!(depositor.is_writable, JackpotError::InvalidRentReceiver);
                    let mut amount: u64 = 0;
                    for deposit in round
                        .deposits_mut()
                        .iter_mut()
                        .filter(|d| d.depositor == depositor.key() && d.refunded == 0)
                    {
                        require!(deposit.asset == ASSET_SOL, JackpotError::UnsupportedAsset);
                        amount = amount
                            .checked_add(deposit.amount)
                            .ok_or(JackpotError::MathOverflow)?;
                        deposit.refunded = 1;
                    }
                    if amount > 0 {
                        refunds.push((i, amount));
                    }
                }
            }
            _ => {}
        }
        round.is_finished()
    };

    if let Some(sol_amount) = prize {
        ctx.accounts.sol_vault.sub_lamports(sol_amount)?;
        ctx.accounts.winner.add_lamports(sol_amount)?;
        emit!(PrizeClaimed {
            round_id,
            winner,
            sol_amount,
            usdc_amount: 0,
        });
    }
    if let Some(sol_amount) = fees {
        ctx.accounts.sol_vault.sub_lamports(sol_amount)?;
        ctx.accounts.treasury.add_lamports(sol_amount)?;
        emit!(FeesWithdrawn {
            round_id,
            treasury: ctx.accounts.treasury.key(),
            sol_amount,
            usdc_amount: 0,
        });
    }
    for (i, amount) in &refunds {
        let depositor = &ctx.remaining_accounts[*i];
        ctx.accounts.sol_vault.sub_lamports(*amount)?;
        depositor.add_lamports(*amount)?;
        emit!(Refunded {
            round_id,
            depositor: depositor.key(),
            sol_amount: *amount,
            usdc_amount: 0,
        });
    }

    if finished {
        // Close both accounts like Anchor's `close`: all lamports to the opener, owner back to the system
        // program. Anchor's exit skips closed accounts.
        let opener = ctx.accounts.opener.to_account_info();
        for info in [
            ctx.accounts.round.to_account_info(),
            ctx.accounts.sol_vault.to_account_info(),
        ] {
            let lamports = info.lamports();
            info.sub_lamports(lamports)?;
            opener.add_lamports(lamports)?;
            info.assign(&anchor_lang::solana_program::system_program::ID);
            info.resize(0)?;
        }
        emit!(RoundAccountsClosed {
            round_id,
            rent_receiver: opener.key(),
        });
    } else {
        require!(acted || !refunds.is_empty(), JackpotError::RoundNotSettled);
    }
    Ok(())
}
