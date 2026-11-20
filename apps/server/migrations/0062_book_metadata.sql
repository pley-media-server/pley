-- What an audiobook was looked up by (see `src/providers/books.rs`): the
-- Audible ASIN its files carry ("B08G9PRS1K"), which Audnexus is asked by,
-- and the Open Library work it was matched to ("OL21745884W"), which a later
-- look-up keeps while it still fits the book.
ALTER TABLE items ADD COLUMN audible_asin TEXT;
ALTER TABLE items ADD COLUMN openlibrary_id TEXT;
