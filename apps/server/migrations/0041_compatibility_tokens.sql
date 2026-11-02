-- Tokens for other apps and tools that use the compatibility API
-- (src/auth/compatibility.rs): Tautulli, Kometa, players and scripts. They
-- work there only, never on Pley's own API, and only for what their scope
-- allows. A token ends when it is revoked, 90 days after it was last used,
-- and with the account's other sign-ins.
CREATE TABLE compatibility_tokens (
    -- Never given again once a token is gone (AUTOINCREMENT): a revocation
    -- from an account page opened before can't end a newer token.
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- What the person called it ("Tautulli"), for the account page.
    name         TEXT    NOT NULL,
    -- What it may do, space-separated: 'play activity'. Always 'play'.
    scope        TEXT    NOT NULL,
    -- SHA-256 of the token, like session tokens: never the token itself.
    token_hash   TEXT    NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    -- NULL until it is first used.
    last_used_at INTEGER,
    -- 90 days after it was made or last used.
    expires_at   INTEGER NOT NULL,
    -- Of its last use, for the account page.
    last_ip      TEXT,
    user_agent   TEXT
);
CREATE INDEX compatibility_tokens_user ON compatibility_tokens(user_id);
