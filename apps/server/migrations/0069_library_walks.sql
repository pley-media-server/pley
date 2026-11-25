-- When each library was last scanned whole. `scanned_at` moves with every
-- scan, those of the folders that changed included; the periodic scan of
-- the whole library is due from this.
ALTER TABLE libraries ADD COLUMN walked_at INTEGER;
UPDATE libraries SET walked_at = scanned_at;
