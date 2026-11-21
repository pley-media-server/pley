-- Signing in with Tailscale (src/tailscale.rs): the Tailscale user each
-- account linked, one each way. A request over the tailnet from a device
-- that user is signed in to signs in as the account, and two-step sign-in
-- still asks its code. A new password removes the link, and signs out the
-- sessions signed in with it.
CREATE TABLE tailscale_links (
    user_id        INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    -- Tailscale's id for the user and the tailnet's ('42@T1234CNTRL'): a
    -- login name may change hands, and ids are a tailnet's own.
    tailscale_user TEXT    NOT NULL UNIQUE,
    -- Their login name when they last linked or signed in, to show.
    login_name     TEXT    NOT NULL,
    created_at     INTEGER NOT NULL
);

-- The Tailscale user a session signed in as, or switched profiles from: it
-- works only over the tailnet from a device of theirs, while signing in
-- with Tailscale is on.
ALTER TABLE sessions ADD COLUMN tailscale_user TEXT;
CREATE INDEX sessions_tailscale_user ON sessions(tailscale_user) WHERE tailscale_user IS NOT NULL;
