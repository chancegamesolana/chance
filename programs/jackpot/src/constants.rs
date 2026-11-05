use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const ROUND_SEED: &[u8] = b"round";

#[constant]
pub const SOL_VAULT_SEED: &[u8] = b"sol_vault";

/// Seed of a player's per-round `Ticket`: `["ticket", round_id, player]`.
#[constant]
pub const TICKET_SEED: &[u8] = b"ticket";

/// Seed of our identity PDA that signs VRF requests; MagicBlock also uses it for the scoped callback identity.
#[constant]
pub const VRF_IDENTITY_SEED: &[u8] = b"identity";

/// Capacity of a legacy `Round`'s fixed deposit array. `RoundV2` has no deposit cap.
pub const LEGACY_MAX_DEPOSITS: usize = 100;

/// Largest account the runtime allows (`MAX_PERMITTED_DATA_LENGTH`, 10 MiB). Bounds a `RoundV2`'s deposits.
pub const MAX_ACCOUNT_DATA_LEN: usize = 10 * 1024 * 1024;

/// Hard cap on `GameConfig::fee_bps` (10%).
#[constant]
pub const MAX_FEE_BPS: u16 = 1_000;

pub const BPS_DENOMINATOR: u64 = 10_000;

// Bounds on the config tunables (security review L-2). They stop an admin typo (or a compromised
// admin key) from wedging rounds: a tiny reveal timeout cancels every round before the oracle can answer, an
// enormous round duration locks deposits for as long as the round lasts.
/// Shortest reveal timeout: the VRF callback normally lands within seconds, so a minute never cancels a live draw.
#[constant]
pub const MIN_REVEAL_TIMEOUT_SECS: u32 = 60;
/// Longest reveal timeout (7 days): also bounds how long a stuck round can hold its deposits before anyone may
/// cancel it (see `RoundV2::cancel_reason`).
#[constant]
pub const MAX_REVEAL_TIMEOUT_SECS: u32 = 7 * 24 * 3600;
/// Longest round (24 hours).
#[constant]
pub const MAX_ROUND_DURATION_SECS: u32 = 24 * 3600;

// Round `status` values.
#[constant]
pub const STATUS_OPEN: u8 = 0;
#[constant]
pub const STATUS_DRAWING: u8 = 1;
#[constant]
pub const STATUS_DRAWN: u8 = 2;
#[constant]
pub const STATUS_CANCELLED: u8 = 3;

// `Deposit::asset` values.
#[constant]
pub const ASSET_SOL: u8 = 0;
#[constant]
pub const ASSET_USDC: u8 = 1;

// `RoundCancelled::reason` values.
#[constant]
pub const CANCEL_NOT_ENOUGH_PARTICIPANTS: u8 = 0;
#[constant]
pub const CANCEL_REVEAL_TIMEOUT: u8 = 1;
/// An open round (two or more participants) that nobody managed to close within `reveal_timeout_secs` of its
/// cutoff, e.g. because the VRF request CPI kept failing. Added in the security review (M-1).
#[constant]
pub const CANCEL_CLOSE_TIMEOUT: u8 = 2;
