-- Profiles the import turned off while it runs: those with labels they
-- never see, until Plex's labels are on Pley's titles. Turned on again
-- when a run finishes, never one an administrator turned off.
ALTER TABLE import_accounts ADD COLUMN held INTEGER NOT NULL DEFAULT 0;
