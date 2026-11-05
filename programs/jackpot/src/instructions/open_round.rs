use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::RoundOpened,
    state::{round_status, GameConfig, RoundV2, SolVault},
};

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct OpenRound<'info> {
    /// Pays rent for the round header and vault accounts. Anyone may open a round.
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, GameConfig>,
    /// CHECK: PDA of round `config.current_round_id`, either layout. Empty if it never existed or was closed
    /// (closing requires a finished round); otherwise read in the handler, which requires Drawn or Cancelled.
    #[account(seeds = [ROUND_SEED, &config.current_round_id.to_le_bytes()], bump)]
    pub previous_round: UncheckedAccount<'info>,
    /// Starts as the header only; each deposit appends one record.
    #[account(
        init,
        payer = payer,
        space = 8 + RoundV2::SIZE,
        seeds = [ROUND_SEED, &round_id.to_le_bytes()],
        bump
    )]
    pub round: AccountLoader<'info, RoundV2>,
    #[account(
        init,
        payer = payer,
        space = 8 + SolVault::INIT_SPACE,
        seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()],
        bump
    )]
    pub sol_vault: Account<'info, SolVault>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_round(ctx: Context<OpenRound>, round_id: u64) -> Result<()> {
    let config = &mut ctx.accounts.config;
    require!(!config.paused, JackpotError::Paused);
    let expected_id = config
        .current_round_id
        .checked_add(1)
        .ok_or(JackpotError::MathOverflow)?;
    require!(round_id == expected_id, JackpotError::InvalidRoundId);
    require!(
        previous_round_finished(&ctx.accounts.previous_round)?,
        JackpotError::PreviousRoundInProgress
    );

    let now = Clock::get()?.unix_timestamp;
    let cutoff_at = now
        .checked_add(i64::from(config.round_duration_secs))
        .ok_or(JackpotError::MathOverflow)?;

    let mut round = ctx.accounts.round.load_init()?;
    round.id = round_id;
    round.opened_at = now;
    round.cutoff_at = cutoff_at;
    round.opener = ctx.accounts.payer.key();
    round.lamports_per_entry = config.lamports_per_entry;
    round.min_deposit_lamports = config.min_deposit_lamports;
    round.max_pot_lamports = config.max_pot_lamports;
    round.deposit_lock_secs = config.deposit_lock_secs;
    round.reveal_timeout_secs = config.reveal_timeout_secs;
    round.fee_bps = config.fee_bps;
    round.status = STATUS_OPEN;
    round.bump = ctx.bumps.round;
    round.sol_vault_bump = ctx.bumps.sol_vault;

    config.current_round_id = round_id;

    emit!(RoundOpened {
        round_id,
        opened_at: now,
        cutoff_at,
        fee_bps: round.fee_bps,
        lamports_per_entry: round.lamports_per_entry,
    });
    Ok(())
}

/// True if the previous round's PDA holds no live round, or holds one (of either layout) that is Drawn or
/// Cancelled.
fn previous_round_finished(info: &AccountInfo) -> Result<bool> {
    if info.owner != &crate::ID || info.data_is_empty() {
        return Ok(true);
    }
    let status = round_status(info).map_err(|_| error!(JackpotError::InconsistentRoundState))?;
    Ok(status == STATUS_DRAWN || status == STATUS_CANCELLED)
}
