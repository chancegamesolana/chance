use anchor_lang::prelude::*;

#[error_code]
pub enum JackpotError {
    #[msg("Signer is not the config admin")]
    Unauthorized,
    #[msg("Signer is not the program upgrade authority")]
    NotUpgradeAuthority,
    #[msg("Invalid config parameter")]
    InvalidConfig,
    #[msg("Fee exceeds MAX_FEE_BPS")]
    FeeTooHigh,
    #[msg("Game is paused")]
    Paused,
    #[msg("Round id must be current_round_id + 1")]
    InvalidRoundId,
    #[msg("Previous round is still in progress")]
    PreviousRoundInProgress,
    #[msg("Round is not open")]
    RoundNotOpen,
    #[msg("Deposit window has closed")]
    DepositWindowClosed,
    #[msg("Deposit is below the minimum USD value")]
    BelowMinDeposit,
    #[msg("Deposit is worth zero entries")]
    ZeroEntries,
    #[msg("Round has reached its deposit cap")]
    MaxDepositsReached,
    #[msg("Round has reached its participant cap")]
    MaxParticipantsReached,
    #[msg("Deposit would exceed the round's pot cap")]
    PotCapExceeded,
    #[msg("Round cannot be closed yet")]
    RoundNotReadyToClose,
    #[msg("Round needs at least two participants to draw")]
    NotEnoughParticipants,
    #[msg("Oracle queue does not match config")]
    InvalidOracleQueue,
    #[msg("Callback is not signed by the VRF identity for this program")]
    InvalidVrfIdentity,
    // Reserved for a future USDC phase (kept so later error codes stay stable on devnet).
    #[msg("Price oracle is not available in this build")]
    OracleUnavailable,
    #[msg("Invalid oracle price")]
    InvalidPrice,
    #[msg("Round is not drawing")]
    RoundNotDrawing,
    #[msg("Round is not drawn")]
    RoundNotDrawn,
    #[msg("Account is not the round winner")]
    NotWinner,
    #[msg("Prize already claimed")]
    PrizeAlreadyClaimed,
    #[msg("Fees already withdrawn")]
    FeesAlreadyWithdrawn,
    #[msg("Treasury account does not match config")]
    InvalidTreasury,
    #[msg("Round cannot be cancelled")]
    CannotCancel,
    #[msg("Round is not cancelled")]
    RoundNotCancelled,
    #[msg("Nothing to refund")]
    NothingToRefund,
    #[msg("Round still holds unclaimed funds")]
    RoundNotSettled,
    #[msg("Asset is not supported yet")]
    UnsupportedAsset,
    #[msg("Round state is inconsistent")]
    InconsistentRoundState,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    // From the first devnet deploy on, add new variants only at the end so error codes stay stable.
    #[msg("Rent receiver must be the round's opener")]
    InvalidRentReceiver,
    // Added with uncapped rounds (RoundV2 + Ticket).
    #[msg("Round account has reached the maximum account size")]
    RoundFull,
    #[msg("Round still has open tickets")]
    TicketsOutstanding,
    #[msg("Ticket does not belong to this round or player")]
    InvalidTicket,
    // Added in the security review: two-step admin transfer.
    #[msg("Signer is not the pending admin")]
    NotPendingAdmin,
}
