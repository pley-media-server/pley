-- no-transaction
-- Audiobooks: libraries of kind 'audiobooks' hold items of kind 'audiobook',
-- each with its files as items of kind 'part'.
--
-- The libraries' kinds are checked by the table itself, so the table is made
-- again with the new kind, as SQLite's documentation says to change a
-- constraint (https://sqlite.org/lang_altertable.html#otheralter): with
-- foreign keys off for the while, so that dropping the old table doesn't
-- delete every library's items, and in one transaction. The trigger on the
-- old table goes with it and is made again.
--
-- A migration without sqlx's transaction is recorded only after it ran, so
-- a crash in between runs it again: it makes the table again and nothing
-- else, which a second run does as well. The books' columns are added by
-- the next migration, in sqlx's transaction.
PRAGMA foreign_keys = OFF;
BEGIN;

CREATE TABLE libraries_new (
    id             INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL,
    kind           TEXT    NOT NULL CHECK (kind IN ('movies', 'shows', 'music', 'audiobooks')),
    paths          TEXT    NOT NULL DEFAULT '[]',
    created_at     INTEGER NOT NULL,
    scanned_at     INTEGER,
    position       INTEGER NOT NULL DEFAULT 0,
    labels_pending INTEGER NOT NULL DEFAULT 0
);
INSERT INTO libraries_new (id, name, kind, paths, created_at, scanned_at, position, labels_pending)
    SELECT id, name, kind, paths, created_at, scanned_at, position, labels_pending FROM libraries;
DROP TABLE libraries;
ALTER TABLE libraries_new RENAME TO libraries;

CREATE TRIGGER library_ids_given AFTER INSERT ON libraries BEGIN
    UPDATE library_ids SET last = MAX(last, NEW.id);
END;

COMMIT;
PRAGMA foreign_keys = ON;
