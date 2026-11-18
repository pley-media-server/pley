-- A `.strm` file names the address its media is at (see `src/strm.rs`).
-- `link_size` is the `.strm` file's own size (NULL for every other file),
-- which with `mtime` says whether it changed; `size` is then its media's,
-- 0 until it is known. `link_key` tells which address the file named when
-- it was last scanned: what was found of its media (its streams, its
-- length, its size) is kept for as long as it names that one.
ALTER TABLE media_files ADD COLUMN link_size INTEGER;
ALTER TABLE media_files ADD COLUMN link_key TEXT;
