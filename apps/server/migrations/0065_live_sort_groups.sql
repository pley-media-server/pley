-- A Live TV source whose groups every app lists A to Z rather than in the
-- order its provider gives them, which some providers give in no order.
ALTER TABLE live_sources ADD COLUMN sort_groups INTEGER NOT NULL DEFAULT 0;
