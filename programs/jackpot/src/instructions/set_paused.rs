use anchor_lang::prelude::*;

use crate::{constants::*, error::JackpotError, state::GameConfig};

/// Pausing blocks `open_round` and deposits only. Closing, settling, claiming, cancelling and refunds keep
/// working so funds already in a round can always leave.
#[derive(Accounts)]
pub struct SetPaused<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ JackpotError::Unauthorized
    )]
    pub config: Account<'info, GameConfig>,
}

pub fn handle_set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    Ok(())
}
