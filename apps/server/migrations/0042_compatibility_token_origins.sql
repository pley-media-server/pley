-- Where a token for other apps may be used from (src/auth/compatibility.rs):
-- addresses and networks, space-separated ('192.168.1.20 10.0.0.0/24'), or
-- 'local' for this computer and the home network. NULL: anywhere.
ALTER TABLE compatibility_tokens ADD COLUMN allowed_from TEXT;
