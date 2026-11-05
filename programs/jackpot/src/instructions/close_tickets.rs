use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::JackpotError,
    events::TicketClosed,
    state::{release_records, RoundV2, Ticket},
};

/// Permissionless: closes tickets of a Drawn round. Remaining accounts are `(ticket, player)` pairs, both
/// writable; each ticket's rent and its player's record rent go only to that ticket's player. Cancelled rounds
/// close tickets through `withdraw_refund`, which also returns the stake. A pair whose ticket is already closed
/// (empty and system-owned, e.g. closed by another cranker between planning and sending, or earlier in this
/// batch) is skipped rather than failing the batch; skipping moves no lamports and counts nothing.
#[derive(Accounts)]
#[instruction(round_id: u64)]
pub struct CloseTickets<'info> {
    #[account(mut, seeds = [ROUND_SEED, &round_id.to_le_bytes()], bump)]
    pub round: AccountLoader<'info, RoundV2>,
}

pub fn handle_close_tickets<'info>(
    ctx: Context<'info, CloseTickets<'info>>,
    round_id: u64,
) -> Result<()> {
    let pairs = ctx.remaining_accounts;
    require!(
        !pairs.is_empty() && pairs.len() % 2 == 0,
        JackpotError::InvalidTicket
    );
    require!(
        ctx.accounts.round.load()?.status == STATUS_DRAWN,
        JackpotError::RoundNotDrawn
    );
    let rent = Rent::get()?;
    let round_info = ctx.accounts.round.to_account_info();
    let mut closed: u32 = 0;
    for pair in pairs.chunks_exact(2) {
        let (ticket_info, player_info) = (&pair[0], &pair[1]);
        if ticket_info.owner == &anchor_lang::solana_program::system_program::ID
            && ticket_info.data_is_empty()
            && ticket_info.lamports() == 0
        {
            continue; // already closed
        }
        // Only this program creates Ticket accounts, and only at their PDA, so a program-owned account with the
        // Ticket discriminator for this round is genuine; the player must be the one it names.
        require_keys_eq!(*ticket_info.owner, crate::ID, JackpotError::InvalidTicket);
        require!(
            ticket_info.is_writable && player_info.is_writable,
            JackpotError::InvalidTicket
        );
        let ticket = {
            let data = ticket_info.try_borrow_data()?;
            Ticket::try_deserialize(&mut &data[..])?
        };
        require!(ticket.round_id == round_id, JackpotError::InvalidTicket);
        require_keys_eq!(
            ticket.player,
            player_info.key(),
            JackpotError::InvalidTicket
        );

        let records_rent = release_records(
            &round_info,
            player_info,
            ticket.deposits,
            ticket.record_rent,
            &rent,
        )?;
        // Close the ticket like Anchor's `close`: all lamports to the player, owner back to the system program.
        let ticket_rent = ticket_info.lamports();
        ticket_info.sub_lamports(ticket_rent)?;
        player_info.add_lamports(ticket_rent)?;
        ticket_info.assign(&anchor_lang::solana_program::system_program::ID);
        ticket_info.resize(0)?;
        closed = closed.checked_add(1).ok_or(JackpotError::MathOverflow)?;

        emit!(TicketClosed {
            round_id,
            player: ticket.player,
            rent_lamports: records_rent
                .checked_add(ticket_rent)
                .ok_or(JackpotError::MathOverflow)?,
        });
    }
    let mut round = ctx.accounts.round.load_mut()?;
    round.tickets_closed = round
        .tickets_closed
        .checked_add(closed)
        .ok_or(JackpotError::MathOverflow)?;
    require!(
        round.tickets_closed <= round.participants_len,
        JackpotError::InconsistentRoundState
    );
    Ok(())
}
