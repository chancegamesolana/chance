use anchor_lang::{prelude::*, system_program};

use crate::{
    constants::*,
    error::JackpotError,
    events::DepositedV2,
    state::{
        write_record, GameConfig, RoundV2, SolVault, Ticket, DEPOSIT_RECORD_SIZE,
        ROUND_V2_HEADER_LEN,
    },
};

#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct DepositSol<'info> {
    /// Pays the deposit, the new deposit record's rent and (first deposit only) the ticket's rent. Both rents
    /// come back when the ticket closes.
    #[account(mut)]
    pub depositor: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, GameConfig>,
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, RoundV2>,
    #[account(mut, seeds = [SOL_VAULT_SEED, &round_id.to_le_bytes()], bump)]
    pub sol_vault: Account<'info, SolVault>,
    /// Created by the player's first deposit in this round. Tickets close only once the round is Drawn or
    /// Cancelled, when no deposit can succeed, so `init_if_needed` can't resurrect a closed ticket: a deposit
    /// into a finished round fails and the whole transaction (including the re-init) is rolled back.
    #[account(
        init_if_needed,
        payer = depositor,
        space = 8 + Ticket::INIT_SPACE,
        seeds = [TICKET_SEED, &round_id.to_le_bytes(), depositor.key().as_ref()],
        bump
    )]
    pub ticket: Account<'info, Ticket>,
    pub system_program: Program<'info, System>,
}

pub fn handle_deposit_sol(ctx: Context<DepositSol>, round_id: u64, lamports: u64) -> Result<()> {
    require!(!ctx.accounts.config.paused, JackpotError::Paused);
    let now = Clock::get()?.unix_timestamp;
    let rent = Rent::get()?;
    let depositor = ctx.accounts.depositor.key();
    let new_participant = ctx.accounts.ticket.player == Pubkey::default();

    let round_info = ctx.accounts.round.to_account_info();
    let old_len = round_info.data_len();
    let new_len = old_len
        .checked_add(DEPOSIT_RECORD_SIZE)
        .ok_or(JackpotError::MathOverflow)?;
    require!(new_len <= MAX_ACCOUNT_DATA_LEN, JackpotError::RoundFull);
    let record_rent = rent
        .minimum_balance(new_len)
        .checked_sub(rent.minimum_balance(old_len))
        .ok_or(JackpotError::MathOverflow)?;

    let outcome = ctx
        .accounts
        .round
        .load_mut()?
        .record_deposit(lamports, now, new_participant)?;

    // One transfer for the stake and the record rent; the rent then moves from the vault (program-owned) to
    // the round account, which grows by one record.
    let total = lamports
        .checked_add(record_rent)
        .ok_or(JackpotError::MathOverflow)?;
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.depositor.to_account_info(),
                to: ctx.accounts.sol_vault.to_account_info(),
            },
        ),
        total,
    )?;
    ctx.accounts.sol_vault.sub_lamports(record_rent)?;
    round_info.add_lamports(record_rent)?;
    round_info.resize(new_len)?;
    {
        let mut data = round_info.try_borrow_mut_data()?;
        let index = usize::try_from(outcome.index).map_err(|_| JackpotError::MathOverflow)?;
        write_record(
            &mut data[ROUND_V2_HEADER_LEN..],
            index,
            &depositor,
            lamports,
            outcome.cumulative_entries,
        )?;
    }

    let ticket = &mut ctx.accounts.ticket;
    if new_participant {
        ticket.round_id = round_id;
        ticket.player = depositor;
        ticket.index = outcome.participant_index;
        ticket.bump = ctx.bumps.ticket;
    }
    ticket.amount = ticket
        .amount
        .checked_add(lamports)
        .ok_or(JackpotError::MathOverflow)?;
    ticket.entries = ticket
        .entries
        .checked_add(outcome.entries)
        .ok_or(JackpotError::MathOverflow)?;
    ticket.record_rent = ticket
        .record_rent
        .checked_add(record_rent)
        .ok_or(JackpotError::MathOverflow)?;
    ticket.deposits = ticket
        .deposits
        .checked_add(1)
        .ok_or(JackpotError::MathOverflow)?;

    emit!(DepositedV2 {
        round_id,
        depositor,
        deposit_index: outcome.index,
        amount: lamports,
        entries: outcome.entries,
        cumulative_entries: outcome.cumulative_entries,
        new_participant,
        participant_index: ticket.index,
        record_rent,
    });
    Ok(())
}
