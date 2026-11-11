-- Libraries an import gave a profile with labels it never sees, opened only
-- when a run finishes: until then Plex's labels aren't on their titles.
CREATE TABLE import_waiting_libraries (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, library_id)
) WITHOUT ROWID;

-- A profile that may open every library once its waiting ones are open.
ALTER TABLE import_accounts ADD COLUMN waits_for_all INTEGER NOT NULL DEFAULT 0;
