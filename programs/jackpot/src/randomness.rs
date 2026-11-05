//! Randomness via MagicBlock VRF.
//!
//! `close_round` requests randomness by CPI, signed by our `["identity"]` PDA. The VRF oracle later calls
//! `settle_round` with 32 random bytes, signed by the *scoped* VRF identity
//! `PDA(["identity", our program id], VRF program)`. The VRF program only produces that signature when
//! fulfilling a request our program made, so a callback carrying it is genuine.
//!
//! Hand-written from `ephemeral-vrf-sdk` 0.17.3 (MIT) instead of depending on it; see DECISIONS.md.
//! The `mock-randomness` build (tests only) skips the CPI and accepts callbacks signed by
//! `MOCK_VRF_IDENTITY`, a fixed test key.

use anchor_lang::{prelude::*, solana_program::pubkey::pubkey, Discriminator};

/// MagicBlock VRF program (same address on devnet and mainnet).
pub const VRF_PROGRAM_ID: Pubkey = pubkey!("Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz");

/// MagicBlock's base-layer oracle queue (same address on devnet and mainnet).
pub const DEFAULT_ORACLE_QUEUE: Pubkey = pubkey!("Cuj97ggrhhidhbu39TijNVqE74xvKJ69gDervRUXAxGh");

pub const SLOT_HASHES_SYSVAR_ID: Pubkey = pubkey!("SysvarS1otHashes111111111111111111111111111");

/// TEST ONLY: callback signer accepted by the `mock-randomness` build. Its secret is the 32-byte seed
/// `[7; 32]`, known to the tests; it is never accepted by the deployable build.
pub const MOCK_VRF_IDENTITY: Pubkey = pubkey!("GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB");

/// Request variant byte: scoped identity, regular priority (`11` would be high priority).
const REQUEST_SCOPED_REGULAR: u8 = 10;

/// Borsh body of a VRF request, after the 8-byte variant header.
#[derive(AnchorSerialize)]
struct RequestRandomness {
    caller_seed: [u8; 32],
    callback_program_id: Pubkey,
    callback_discriminator: Vec<u8>,
    callback_accounts_metas: Vec<SerializableAccountMeta>,
    callback_args: Vec<u8>,
}

#[derive(AnchorSerialize)]
struct SerializableAccountMeta {
    pubkey: Pubkey,
    is_signer: bool,
    is_writable: bool,
}

/// Seed unique to this round's draw. The VRF program also mixes in recent slot hashes.
pub fn caller_seed(round_id: u64, total_entries: u64, slot: u64) -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[..8].copy_from_slice(&round_id.to_le_bytes());
    seed[8..16].copy_from_slice(&total_entries.to_le_bytes());
    seed[16..24].copy_from_slice(&slot.to_le_bytes());
    seed[24..].copy_from_slice(b"jackpot\0");
    seed
}

/// Instruction data for a request whose callback is `settle_round(randomness, round_id)` on `round`.
pub fn request_data(caller_seed: [u8; 32], round: Pubkey, round_id: u64) -> Result<Vec<u8>> {
    let mut data = vec![REQUEST_SCOPED_REGULAR, 0, 0, 0, 0, 0, 0, 0];
    RequestRandomness {
        caller_seed,
        callback_program_id: crate::ID,
        callback_discriminator: crate::instruction::SettleRound::DISCRIMINATOR.to_vec(),
        callback_accounts_metas: vec![SerializableAccountMeta {
            pubkey: round,
            is_signer: false,
            is_writable: true,
        }],
        callback_args: round_id.to_le_bytes().to_vec(),
    }
    .serialize(&mut data)?;
    Ok(data)
}

/// The signer a genuine `settle_round` callback must carry.
#[cfg(not(feature = "mock-randomness"))]
pub fn callback_identity() -> Pubkey {
    Pubkey::find_program_address(
        &[crate::constants::VRF_IDENTITY_SEED, crate::ID.as_ref()],
        &VRF_PROGRAM_ID,
    )
    .0
}

#[cfg(feature = "mock-randomness")]
pub fn callback_identity() -> Pubkey {
    MOCK_VRF_IDENTITY
}

/// Accounts the VRF request CPI needs, in the order the VRF program expects.
pub struct RequestAccounts<'a, 'info> {
    pub payer: &'a AccountInfo<'info>,
    pub program_identity: &'a AccountInfo<'info>,
    pub oracle_queue: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub slot_hashes: &'a AccountInfo<'info>,
}

/// Sends the VRF request, signed by our `["identity"]` PDA.
#[cfg(not(feature = "mock-randomness"))]
pub fn request(accounts: RequestAccounts, identity_bump: u8, data: Vec<u8>) -> Result<()> {
    use anchor_lang::solana_program::{
        instruction::{AccountMeta, Instruction},
        program::invoke_signed,
    };

    let ix = Instruction {
        program_id: VRF_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(accounts.payer.key(), true),
            AccountMeta::new_readonly(accounts.program_identity.key(), true),
            AccountMeta::new(accounts.oracle_queue.key(), false),
            AccountMeta::new_readonly(accounts.system_program.key(), false),
            AccountMeta::new_readonly(accounts.slot_hashes.key(), false),
        ],
        data,
    };
    invoke_signed(
        &ix,
        &[
            accounts.payer.clone(),
            accounts.program_identity.clone(),
            accounts.oracle_queue.clone(),
            accounts.system_program.clone(),
            accounts.slot_hashes.clone(),
        ],
        &[&[crate::constants::VRF_IDENTITY_SEED, &[identity_bump]]],
    )?;
    Ok(())
}

/// TEST ONLY: LiteSVM has no VRF oracle, so the mock build records the request without sending it.
#[cfg(feature = "mock-randomness")]
pub fn request(_accounts: RequestAccounts, _identity_bump: u8, _data: Vec<u8>) -> Result<()> {
    Ok(())
}

/// The value used for winner selection: the first 8 bytes of the VRF output, little-endian.
pub fn random_value(randomness: &[u8; 32]) -> u64 {
    let mut first = [0u8; 8];
    first.copy_from_slice(&randomness[..8]);
    u64::from_le_bytes(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_data_layout() {
        let round = Pubkey::new_from_array([9; 32]);
        let seed = caller_seed(5, 1234, 777);
        let data = request_data(seed, round, 5).unwrap();

        assert_eq!(&data[..8], &[10, 0, 0, 0, 0, 0, 0, 0]);
        let mut o = 8;
        assert_eq!(&data[o..o + 32], &seed);
        o += 32;
        assert_eq!(&data[o..o + 32], crate::ID.as_ref());
        o += 32;
        // Vec<u8> discriminator: u32 length then bytes.
        assert_eq!(&data[o..o + 4], &8u32.to_le_bytes());
        o += 4;
        assert_eq!(
            &data[o..o + 8],
            crate::instruction::SettleRound::DISCRIMINATOR
        );
        o += 8;
        // One account meta: pubkey, is_signer, is_writable.
        assert_eq!(&data[o..o + 4], &1u32.to_le_bytes());
        o += 4;
        assert_eq!(&data[o..o + 32], round.as_ref());
        o += 32;
        assert_eq!(&data[o..o + 2], &[0, 1]);
        o += 2;
        // Callback args: round id.
        assert_eq!(&data[o..o + 4], &8u32.to_le_bytes());
        o += 4;
        assert_eq!(&data[o..o + 8], &5u64.to_le_bytes());
        assert_eq!(data.len(), o + 8);
    }

    #[test]
    fn caller_seed_is_unique_per_round_state() {
        assert_ne!(caller_seed(1, 100, 50), caller_seed(2, 100, 50));
        assert_ne!(caller_seed(1, 100, 50), caller_seed(1, 101, 50));
        assert_ne!(caller_seed(1, 100, 50), caller_seed(1, 100, 51));
    }

    #[test]
    fn random_value_reads_first_eight_bytes() {
        let mut r = [0xffu8; 32];
        r[..8].copy_from_slice(&42u64.to_le_bytes());
        assert_eq!(random_value(&r), 42);
    }
}
