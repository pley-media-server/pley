-- Who wrote and who reads a book, and the series it is part of, come from
-- the files' tags or folders and are kept on the book: `authors` and
-- `narrators` are JSON arrays of names, in their order; `series_index` is
-- the book's place in its series (2, or 2.5 for a novella between two
-- books).
ALTER TABLE items ADD COLUMN authors TEXT;
ALTER TABLE items ADD COLUMN narrators TEXT;
ALTER TABLE items ADD COLUMN series TEXT;
ALTER TABLE items ADD COLUMN series_index REAL;
