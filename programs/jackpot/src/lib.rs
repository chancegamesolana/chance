pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod math;
pub mod randomness;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("jhEGnMU7RECUH15Z3Yo6psmURzSj8CXBDJcpZkjNYrq");

#[program]
pub mod jackpot {
    use super::*;

    /// Creates the global config. Signer must be the program's upgrade authority.
    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        params: InitConfigParams,
    ) -> Result<()> {
        instructions::initialize_config::handle_initialize_config(ctx, params)
    }

    /// Admin-only. Updates tunables; round-shaping values apply from the next round. `admin` only proposes a
    /// new admin, who takes over with `accept_admin`.
    pub fn update_config(ctx: Context<UpdateConfig>, params: UpdateConfigParams) -> Result<()> {
        instructions::update_config::handle_update_config(ctx, params)
    }

    /// Signed by the pending admin proposed through `update_config`; makes it the admin.
    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::accept_admin::handle_accept_admin(ctx)
    }

    /// Admin-only. Blocks `open_round` and deposits while set.
    pub fn set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
        instructions::set_paused::handle_set_paused(ctx, paused)
    }

    /// Permissionless. Opens round `current_round_id + 1` once the previous round is Drawn or Cancelled.
    pub fn open_round(ctx: Context<OpenRound>, round_id: u64) -> Result<()> {
        instructions::open_round::handle_open_round(ctx, round_id)
    }

    /// Deposits `lamports` into an open round; entries are `lamports / lamports_per_entry`. The depositor also
    /// pays the rent of one deposit record (and of their ticket on their first deposit), returned when their
    /// ticket closes. No cap on deposits or participants.
    pub fn deposit_sol(ctx: Context<DepositSol>, round_id: u64, lamports: u64) -> Result<()> {
        instructions::deposit_sol::handle_deposit_sol(ctx, round_id, lamports)
    }

    /// Permissionless. Stops deposits and requests MagicBlock VRF randomness, whose callback is
    /// `settle_round`. Allowed at cutoff, or early once the pot cap leaves no room for another minimum deposit
    /// (legacy rounds: once one of their caps is hit); needs at least two participants. Accepts legacy rounds.
    pub fn close_round(ctx: Context<CloseRound>, round_id: u64) -> Result<()> {
        instructions::close_round::handle_close_round(ctx, round_id)
    }

    /// MagicBlock VRF callback (signed by the scoped VRF identity). Picks the winner from the delivered
    /// randomness and computes fees; round becomes Drawn.
    pub fn settle_round(
        ctx: Context<SettleRound>,
        randomness: [u8; 32],
        round_id: u64,
    ) -> Result<()> {
        instructions::settle_round::handle_settle_round(ctx, randomness, round_id)
    }

    /// Permissionless. Pays the pot minus fees to the round winner.
    pub fn claim_prize(ctx: Context<ClaimPrize>, round_id: u64) -> Result<()> {
        instructions::claim_prize::handle_claim_prize(ctx, round_id)
    }

    /// Permissionless. Sends a drawn round's fees to the configured treasury (parking them in the config account
    /// while the treasury cannot take a sub-rent credit; forwarded with a later fee).
    pub fn withdraw_fees(ctx: Context<WithdrawFees>, round_id: u64) -> Result<()> {
        instructions::withdraw_fees::handle_withdraw_fees(ctx, round_id)
    }

    /// Permissionless. Cancels an open round that reached its cutoff with fewer than two participants, an open
    /// round nobody could close within the reveal timeout of its cutoff, or a drawing round whose randomness
    /// did not arrive within the reveal timeout of its close.
    pub fn cancel_round(ctx: Context<CancelRound>, round_id: u64) -> Result<()> {
        instructions::cancel_round::handle_cancel_round(ctx, round_id)
    }

    /// Permissionless; funds go only to the ticket's player. Refunds everything the player deposited in a
    /// cancelled round and closes their ticket (its rent and their record rent go back to them).
    pub fn withdraw_refund(ctx: Context<WithdrawRefund>, round_id: u64) -> Result<()> {
        instructions::withdraw_refund::handle_withdraw_refund(ctx, round_id)
    }

    /// Permissionless. Closes tickets of a drawn round, given as `(ticket, player)` remaining-account pairs;
    /// each ticket's rent and its player's record rent go back to that player. Pairs whose ticket is already
    /// closed are skipped, so a batch is never spoiled by someone closing one of its tickets first.
    pub fn close_tickets<'info>(
        ctx: Context<'info, CloseTickets<'info>>,
        round_id: u64,
    ) -> Result<()> {
        instructions::close_tickets::handle_close_tickets(ctx, round_id)
    }

    /// Permissionless. Closes a finished round (every ticket closed) and its vault; rent returns to the opener.
    pub fn close_round_accounts(ctx: Context<CloseRoundAccounts>, round_id: u64) -> Result<()> {
        instructions::close_round_accounts::handle_close_round_accounts(ctx, round_id)
    }

    /// Permissionless. Finishes a legacy round (opened before the RoundV2 upgrade) under the legacy rules:
    /// cancels it if allowed, pays prize and fees, refunds the depositors given as remaining accounts, and
    /// closes it (rent to the opener) once it owes nothing. See instructions/legacy.rs.
    pub fn legacy_finish_round<'info>(
        ctx: Context<'info, LegacyFinishRound<'info>>,
        round_id: u64,
    ) -> Result<()> {
        instructions::legacy::handle_legacy_finish_round(ctx, round_id)
    }
}
