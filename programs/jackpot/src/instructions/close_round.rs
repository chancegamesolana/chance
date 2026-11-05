use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::{RoundClosed, RoundClosedV2},
    randomness::{self, RequestAccounts},
    state::{legacy_round_mut, round_layout, split_round_v2, GameConfig, RoundLayout},
};

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct CloseRound<'info> {
    /// Anyone may close a round; the payer covers the VRF request.
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, GameConfig>,
    /// CHECK: round `round_id` of either layout (`RoundV2`, or a legacy `Round` opened before the upgrade);
    /// owner and discriminator are checked by `round_layout` in the handler.
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: UncheckedAccount<'info>,
    /// CHECK: our identity PDA; signs the VRF request via invoke_signed and holds no data.
    #[account(seeds = [VRF_IDENTITY_SEED], bump)]
    pub program_identity: UncheckedAccount<'info>,
    /// CHECK: the configured MagicBlock oracle queue; validated by address, written by the VRF program.
    #[account(mut, address = config.vrf_oracle_queue @ JackpotError::InvalidOracleQueue)]
    pub oracle_queue: UncheckedAccount<'info>,
    /// CHECK: the MagicBlock VRF program, validated by address.
    #[account(address = randomness::VRF_PROGRAM_ID)]
    pub vrf_program: UncheckedAccount<'info>,
    /// CHECK: SlotHashes sysvar, validated by address; the VRF program mixes it into the request.
    #[account(address = randomness::SLOT_HASHES_SYSVAR_ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_close_round(ctx: Context<CloseRound>, round_id: u64) -> Result<()> {
    let clock = Clock::get()?;
    let queue = ctx.accounts.oracle_queue.key();
    let round_info = ctx.accounts.round.to_account_info();
    let layout = round_layout(&round_info)?;
    let (total_entries, participants, deposits) = {
        let mut data = round_info.try_borrow_mut_data()?;
        match layout {
            RoundLayout::V2 => {
                let (round, _) = split_round_v2(&mut data);
                require!(round.status == STATUS_OPEN, JackpotError::RoundNotOpen);
                require!(
                    round.ready_to_close(clock.unix_timestamp),
                    JackpotError::RoundNotReadyToClose
                );
                require!(
                    round.participants_len >= 2,
                    JackpotError::NotEnoughParticipants
                );
                require!(
                    round.total_entries > 0,
                    JackpotError::InconsistentRoundState
                );
                round.vrf_queue = queue;
                round.closed_slot = clock.slot;
                round.closed_at = clock.unix_timestamp;
                round.status = STATUS_DRAWING;
                (
                    round.total_entries,
                    round.participants_len,
                    round.deposits_len,
                )
            }
            RoundLayout::Legacy => {
                let round = legacy_round_mut(&mut data);
                require!(round.status == STATUS_OPEN, JackpotError::RoundNotOpen);
                require!(
                    round.ready_to_close(clock.unix_timestamp),
                    JackpotError::RoundNotReadyToClose
                );
                require!(
                    round.participants_len >= 2,
                    JackpotError::NotEnoughParticipants
                );
                require!(
                    round.total_entries > 0,
                    JackpotError::InconsistentRoundState
                );
                round.vrf_queue = queue;
                round.closed_slot = clock.slot;
                round.closed_at = clock.unix_timestamp;
                round.status = STATUS_DRAWING;
                (
                    round.total_entries,
                    u32::from(round.participants_len),
                    u32::from(round.deposits_len),
                )
            }
        }
    };
    let caller_seed = randomness::caller_seed(round_id, total_entries, clock.slot);

    // Requested only now that deposits are closed, so nobody can know the outcome while they could still bet.
    // The callback's account list is just this round account, whatever its size or layout.
    randomness::request(
        RequestAccounts {
            payer: &ctx.accounts.payer,
            program_identity: &ctx.accounts.program_identity,
            oracle_queue: &ctx.accounts.oracle_queue,
            system_program: &ctx.accounts.system_program,
            slot_hashes: &ctx.accounts.slot_hashes,
        },
        ctx.bumps.program_identity,
        randomness::request_data(caller_seed, round_info.key(), round_id)?,
    )?;

    match layout {
        RoundLayout::V2 => emit!(RoundClosedV2 {
            round_id,
            vrf_queue: queue,
            caller_seed,
            closed_slot: clock.slot,
            total_entries,
            participants,
            deposits,
        }),
        RoundLayout::Legacy => emit!(RoundClosed {
            round_id,
            vrf_queue: queue,
            caller_seed,
            closed_slot: clock.slot,
            total_entries,
            participants: u16::try_from(participants).map_err(|_| JackpotError::MathOverflow)?,
            deposits: u16::try_from(deposits).map_err(|_| JackpotError::MathOverflow)?,
        }),
    }
    Ok(())
}
