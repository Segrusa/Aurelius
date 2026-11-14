-- v0.27 (protocol 5, feature `ticket_history`, ticket #57): the History of finished tickets.
--
-- * tickets.summary_state: where the summary of the ticket's latest Done stands.
--   NULL: never on Done since the History (or on Done before it, without a summary);
--   'pending': it went to Done and its extraction should bring the summary;
--   'ready': summary_text holds it; 'failed': the extraction failed or gave none
--   (`ticket.retry_summary` tries again). Every Done sets it to 'pending' again.
-- * tickets.summary_text: the summary the extraction writes into the project's memory
--   (MU1-18), without its "Ticket #n … is done" head, through the vault redactor.
--
-- Additive only: new columns with CHECKs, no rebuild. The step after this SQL
-- (`history::backfill`) fills summary_text for tickets already on Done from the
-- summary in their project's memory (locator.ticket_id), without a model call.
ALTER TABLE tickets ADD COLUMN summary_state TEXT CHECK(summary_state IS NULL OR summary_state IN ('pending','ready','failed'));
ALTER TABLE tickets ADD COLUMN summary_text TEXT CHECK(summary_text IS NULL OR (summary_state = 'ready' AND length(summary_text) BETWEEN 1 AND 800));
CREATE INDEX tickets_done_at ON tickets(done_at_ms DESC, number DESC) WHERE done_at_ms IS NOT NULL;
