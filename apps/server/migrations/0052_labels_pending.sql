-- Libraries a Plex import is bringing Plex's labels to: a profile with
-- labels it never sees doesn't see them until a run that brings them
-- finishes, since until then its rules would hide nothing there.
ALTER TABLE libraries ADD COLUMN labels_pending INTEGER NOT NULL DEFAULT 0;
