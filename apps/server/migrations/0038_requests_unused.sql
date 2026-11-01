-- Requests (src/requests): what 0037 kept for sends tried again by
-- themselves, for the stage Radarr or Sonarr would say and for managers'
-- notes, which nothing writes: a failed request waits for a manager, an
-- approved one's stage comes from its download, and "Added by hand" from
-- `sent_to`.
DROP INDEX requests_retry;
ALTER TABLE requests DROP COLUMN sent_seasons;
ALTER TABLE requests DROP COLUMN attempts;
ALTER TABLE requests DROP COLUMN retry_at;
ALTER TABLE requests DROP COLUMN stage;
ALTER TABLE requests DROP COLUMN stage_at;
ALTER TABLE requests DROP COLUMN note;
