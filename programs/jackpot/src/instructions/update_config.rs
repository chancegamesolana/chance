use anchor_lang::prelude::*;

use crate::{constants::*, error::JackpotError, state::GameConfig};

/// Fields left as `None` are unchanged. `usdc_mint` and `max_deposits` are fixed at initialization.
/// Round-shaping fields take effect from the next `open_round`. `max_participants` is reserved: rounds have no
/// participant cap, so it is ignored (kept so the instruction's argument layout is unchanged).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct UpdateConfigParams {
    /// Proposes a new admin (two-step): it becomes `pending_admin` and takes over only when it
    /// signs `accept_admin`, so a typo cannot lose the admin key. The default pubkey withdraws a pending proposal.
    pub admin: Option<Pubkey>,
    pub treasury: Option<Pubkey>,
    pub pyth_sol_usd_feed: Option<Pubkey>,
    pub vrf_oracle_queue: Option<Pubkey>,
    pub fee_bps: Option<u16>,
    pub lamports_per_entry: Option<u64>,
    pub round_duration_secs: Option<u32>,
    pub deposit_lock_secs: Option<u32>,
    pub reveal_timeout_secs: Option<u32>,
    pub max_participants: Option<u16>,
    pub min_deposit_lamports: Option<u64>,
    pub max_price_age_secs: Option<u32>,
    pub max_pot_lamports: Option<u64>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ JackpotError::Unauthorized
    )]
    pub config: Account<'info, GameConfig>,
}

pub fn handle_update_config(ctx: Context<UpdateConfig>, params: UpdateConfigParams) -> Result<()> {
    let c = &mut ctx.accounts.config;
    if let Some(v) = params.admin {
        c.pending_admin = v;
    }
    if let Some(v) = params.treasury {
        c.treasury = v;
    }
    if let Some(v) = params.pyth_sol_usd_feed {
        c.pyth_sol_usd_feed = v;
    }
    if let Some(v) = params.vrf_oracle_queue {
        c.vrf_oracle_queue = v;
    }
    if let Some(v) = params.fee_bps {
        c.fee_bps = v;
    }
    if let Some(v) = params.lamports_per_entry {
        c.lamports_per_entry = v;
    }
    if let Some(v) = params.round_duration_secs {
        c.round_duration_secs = v;
    }
    if let Some(v) = params.deposit_lock_secs {
        c.deposit_lock_secs = v;
    }
    if let Some(v) = params.reveal_timeout_secs {
        c.reveal_timeout_secs = v;
    }
    if let Some(v) = params.min_deposit_lamports {
        c.min_deposit_lamports = v;
    }
    if let Some(v) = params.max_price_age_secs {
        c.max_price_age_secs = v;
    }
    if let Some(v) = params.max_pot_lamports {
        c.max_pot_lamports = v;
    }
    c.validate()
}
