-- How a token for other apps was made (src/auth/compatibility.rs), which
-- decides whether it may allow another Jellyfin app with Quick Connect:
-- 'password' (a Jellyfin app's password sign-in) and 'link' (an app allowed
-- on Pley's /link page) may; 'app' (an app allowed by another app's token)
-- and NULL (made on the account page, for a tool, or before this was kept)
-- may not.
ALTER TABLE compatibility_tokens ADD COLUMN signed_in_with TEXT;
-- The token that allowed an 'app' token: revoking it ends this one too.
-- Ids are never given again (AUTOINCREMENT), so one left behind by a token
-- that expired names nothing.
ALTER TABLE compatibility_tokens ADD COLUMN approved_by INTEGER;
CREATE INDEX compatibility_tokens_approved_by ON compatibility_tokens(approved_by);
