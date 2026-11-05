-- The language each account sees Pley in (see src/i18n.rs): an ISO 639-1
-- code; NULL: the server's.
ALTER TABLE users ADD COLUMN language TEXT;
