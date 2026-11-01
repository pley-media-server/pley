-- Requests being sent to Radarr or Sonarr (src/requests): since when, and
-- what their approval chose instead of the settings (the quality profile's
-- id there, and the root folder), so that a send a restart cut short is
-- made again as it was approved. NULL once sent, failed or no longer to be.
ALTER TABLE requests ADD COLUMN sending_at INTEGER;
ALTER TABLE requests ADD COLUMN send_profile INTEGER;
ALTER TABLE requests ADD COLUMN send_folder TEXT;
