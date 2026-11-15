-- The profiles an import made or matched, by the account they were in the
-- other server (Plex's user id): a second run finds the same profile, even
-- renamed, rather than making another.
CREATE TABLE import_accounts (
    source      TEXT    NOT NULL,
    external_id TEXT    NOT NULL,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (source, external_id)
) WITHOUT ROWID;
