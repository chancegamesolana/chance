use anchor_lang::prelude::*;

#[event]
pub struct RoundOpened {
    pub round_id: u64,
    pub opened_at: i64,
    pub cutoff_at: i64,
    pub fee_bps: u16,
    pub lamports_per_entry: u64,
}

#[event]
pub struct Deposited {
    pub round_id: u64,
    pub depositor: Pubkey,
    pub deposit_index: u16,
    pub asset: u8,
    pub amount: u64,
    pub value: u64,
    pub entries: u64,
    pub cumulative_entries: u64,
    pub new_participant: bool,
}

#[event]
pub struct RoundClosed {
    pub round_id: u64,
    pub vrf_queue: Pubkey,
    /// Seed sent with the VRF request; with the oracle's proof it lets anyone verify the draw.
    pub caller_seed: [u8; 32],
    pub closed_slot: u64,
    pub total_entries: u64,
    pub participants: u16,
    pub deposits: u16,
}

#[event]
pub struct RoundSettled {
    pub round_id: u64,
    pub winner: Pubkey,
    pub winner_index: u16,
    /// Full 32-byte VRF output; `random_value` is its first 8 bytes, little-endian.
    pub randomness: [u8; 32],
    pub random_value: u64,
    pub winning_entry: u64,
    pub total_entries: u64,
    /// Pot value in lamports (equals `pot_sol` while SOL is the only asset).
    pub pot_value: u64,
    pub pot_sol: u64,
    pub pot_usdc: u64,
    pub fee_sol: u64,
    pub fee_usdc: u64,
}

#[event]
pub struct PrizeClaimed {
    pub round_id: u64,
    pub winner: Pubkey,
    pub sol_amount: u64,
    pub usdc_amount: u64,
}

#[event]
pub struct FeesWithdrawn {
    pub round_id: u64,
    pub treasury: Pubkey,
    pub sol_amount: u64,
    pub usdc_amount: u64,
}

#[event]
pub struct RoundCancelled {
    pub round_id: u64,
    /// `CANCEL_NOT_ENOUGH_PARTICIPANTS`, `CANCEL_REVEAL_TIMEOUT` or `CANCEL_CLOSE_TIMEOUT`.
    pub reason: u8,
}

#[event]
pub struct Refunded {
    pub round_id: u64,
    pub depositor: Pubkey,
    pub sol_amount: u64,
    pub usdc_amount: u64,
}

#[event]
pub struct RoundAccountsClosed {
    pub round_id: u64,
    pub rent_receiver: Pubkey,
}

// ---- RoundV2 events (rounds opened since the RoundV2 upgrade). Legacy rounds keep emitting the events
// above, whose layouts can't change (stored history decodes with them). Indexes and counts are u32 because
// rounds have no deposit cap.

#[event]
pub struct DepositedV2 {
    pub round_id: u64,
    pub depositor: Pubkey,
    pub deposit_index: u32,
    /// Lamports (SOL is the only asset; a deposit's value equals its amount).
    pub amount: u64,
    pub entries: u64,
    pub cumulative_entries: u64,
    pub new_participant: bool,
    /// The depositor's join order within the round (0-based).
    pub participant_index: u32,
    /// Lamports the depositor paid for the new deposit record (returned when their ticket closes).
    pub record_rent: u64,
}

#[event]
pub struct RoundClosedV2 {
    pub round_id: u64,
    pub vrf_queue: Pubkey,
    /// Seed sent with the VRF request; with the oracle's proof it lets anyone verify the draw.
    pub caller_seed: [u8; 32],
    pub closed_slot: u64,
    pub total_entries: u64,
    pub participants: u32,
    pub deposits: u32,
}

#[event]
pub struct RoundSettledV2 {
    pub round_id: u64,
    pub winner: Pubkey,
    pub winner_index: u32,
    /// Full 32-byte VRF output; `random_value` is its first 8 bytes, little-endian.
    pub randomness: [u8; 32],
    pub random_value: u64,
    pub winning_entry: u64,
    pub total_entries: u64,
    /// Pot in lamports.
    pub pot_sol: u64,
    pub fee_sol: u64,
}

/// A player's ticket was closed (by `withdraw_refund` or `close_tickets`); its rent and the player's record
/// rent went back to them.
#[event]
pub struct TicketClosed {
    pub round_id: u64,
    pub player: Pubkey,
    /// Ticket rent plus returned record rent, in lamports.
    pub rent_lamports: u64,
}
