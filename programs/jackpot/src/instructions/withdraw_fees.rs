use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::FeesWithdrawn,
    state::{GameConfig, RoundV2, SolVault},
};

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct WithdrawFees<'info> {
    /// Also holds fees the treasury could not take yet (`pending_fee_lamports`).
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = treasury @ JackpotError::InvalidTreasury
    )]
    pub config: Account<'info, GameConfig>,
    /// CHECK: must equal `config.treasury` (has_one above); only receives lamports.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, RoundV2>,
    #[account(mut, seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()], bump)]
    pub sol_vault: Account<'info, SolVault>,
}

/// Moves the round's fee out of its vault and marks it claimed, so the round can always finish. The fee goes to
/// the treasury, together with any fees parked earlier, when the treasury can take the credit; otherwise (the
/// runtime rejects a credit that leaves an account below the rent-exempt minimum, e.g. a small fee into an empty
/// treasury) it is parked in the config account until a later `withdraw_fees` can forward it.
pub fn handle_withdraw_fees(ctx: Context<WithdrawFees>, round_id: u64) -> Result<()> {
    let fee = {
        let mut round = ctx.accounts.round.load_mut()?;
        require!(round.status == STATUS_DRAWN, JackpotError::RoundNotDrawn);
        require!(round.fees_claimed == 0, JackpotError::FeesAlreadyWithdrawn);
        round.fees_claimed = 1;
        round.fee_sol_lamports
    };

    let rent = Rent::get()?;
    let treasury = &ctx.accounts.treasury;
    let config_info = ctx.accounts.config.to_account_info();
    let parked = ctx.accounts.config.pending_fee_lamports;
    let due = fee.checked_add(parked).ok_or(JackpotError::MathOverflow)?;
    let after = treasury
        .lamports()
        .checked_add(due)
        .ok_or(JackpotError::MathOverflow)?;
    let treasury_can_take = due == 0 || after >= rent.minimum_balance(treasury.data_len());

    let mut treasury_amount = 0;
    if treasury_can_take {
        if fee > 0 {
            ctx.accounts.sol_vault.sub_lamports(fee)?;
            treasury.add_lamports(fee)?;
        }
        // Never take the config below its own rent (unreachable unless its lamports were tampered with).
        let sweep = parked.min(
            config_info
                .lamports()
                .saturating_sub(rent.minimum_balance(config_info.data_len())),
        );
        if sweep > 0 {
            config_info.sub_lamports(sweep)?;
            treasury.add_lamports(sweep)?;
        }
        ctx.accounts.config.pending_fee_lamports = parked
            .checked_sub(sweep)
            .ok_or(JackpotError::MathOverflow)?;
        treasury_amount = fee.checked_add(sweep).ok_or(JackpotError::MathOverflow)?;
    } else if fee > 0 {
        ctx.accounts.sol_vault.sub_lamports(fee)?;
        config_info.add_lamports(fee)?;
        ctx.accounts.config.pending_fee_lamports = due;
        msg!(
            "treasury below rent-exempt minimum; {} lamports parked in config",
            due
        );
    }

    emit!(FeesWithdrawn {
        round_id,
        treasury: treasury.key(),
        // This round's fee, whichever account holds it now (parked fees reach the treasury with a later round).
        sol_amount: fee,
        usdc_amount: 0,
    });
    if treasury_amount != fee {
        msg!("treasury received {} lamports", treasury_amount);
    }
    Ok(())
}
