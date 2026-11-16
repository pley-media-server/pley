-- Single sign-on (src/sso.rs), back: accounts at an OpenID Connect
-- provider, each linked to one pley user. An identity is the provider (its
-- issuer) and the provider's id for the person (the ID token's `sub`);
-- names and e-mail addresses are never used to match accounts. Identities
-- linked before 0022 took single sign-on out stay gone: they are linked
-- again from the Account page.
CREATE TABLE user_identities (
    issuer       TEXT    NOT NULL,
    subject      TEXT    NOT NULL,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- What the provider said, to show which account is linked.
    email        TEXT,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER,
    PRIMARY KEY (issuer, subject)
) WITHOUT ROWID;
CREATE INDEX user_identities_user ON user_identities(user_id);
