use anchor_lang::prelude::*;

use crate::{constants::*, error::JackpotError, state::GameConfig};

/// Second step of an admin transfer: the key the admin proposed with
/// `update_config { admin }` signs to take over. Until then the current admin keeps full control and can
/// withdraw the proposal by proposing the default pubkey.
#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub new_admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.pending_admin != Pubkey::default() @ JackpotError::NotPendingAdmin,
        constraint = config.pending_admin == new_admin.key() @ JackpotError::NotPendingAdmin,
    )]
    pub config: Account<'info, GameConfig>,
}

pub fn handle_accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    msg!("admin {} -> {}", c.admin, c.pending_admin);
    c.admin = c.pending_admin;
    c.pending_admin = Pubkey::default();
    c.validate()
}
