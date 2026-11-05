use anchor_lang::{prelude::*, solana_program::bpf_loader_upgradeable};

use crate::{constants::*, error::JackpotError, state::GameConfig};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitConfigParams {
    pub treasury: Pubkey,
    pub usdc_mint: Pubkey,
    pub pyth_sol_usd_feed: Pubkey,
    pub vrf_oracle_queue: Pubkey,
    pub fee_bps: u16,
    pub lamports_per_entry: u64,
    pub round_duration_secs: u32,
    pub deposit_lock_secs: u32,
    pub reveal_timeout_secs: u32,
    /// Reserved (stored, not enforced): rounds have no deposit cap.
    pub max_deposits: u16,
    /// Reserved (stored, not enforced): rounds have no participant cap.
    pub max_participants: u16,
    pub min_deposit_lamports: u64,
    pub max_price_age_secs: u32,
    pub max_pot_lamports: u64,
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + GameConfig::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, GameConfig>,
    /// Only the program's upgrade authority may initialize, so nobody can front-run setup after deploy.
    #[account(
        seeds = [crate::ID.as_ref()],
        bump,
        seeds::program = bpf_loader_upgradeable::ID,
        constraint = program_data.upgrade_authority_address == Some(admin.key())
            @ JackpotError::NotUpgradeAuthority,
    )]
    pub program_data: Account<'info, ProgramData>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_config(
    ctx: Context<InitializeConfig>,
    params: InitConfigParams,
) -> Result<()> {
    ctx.accounts.config.set_inner(GameConfig {
        admin: ctx.accounts.admin.key(),
        treasury: params.treasury,
        usdc_mint: params.usdc_mint,
        pyth_sol_usd_feed: params.pyth_sol_usd_feed,
        vrf_oracle_queue: params.vrf_oracle_queue,
        fee_bps: params.fee_bps,
        lamports_per_entry: params.lamports_per_entry,
        round_duration_secs: params.round_duration_secs,
        deposit_lock_secs: params.deposit_lock_secs,
        reveal_timeout_secs: params.reveal_timeout_secs,
        max_deposits: params.max_deposits,
        max_participants: params.max_participants,
        min_deposit_lamports: params.min_deposit_lamports,
        max_price_age_secs: params.max_price_age_secs,
        max_pot_lamports: params.max_pot_lamports,
        paused: false,
        current_round_id: 0,
        bump: ctx.bumps.config,
        pending_admin: Pubkey::default(),
        pending_fee_lamports: 0,
        _reserved: [0; 24],
    });
    ctx.accounts.config.validate()
}
