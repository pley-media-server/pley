-- A poster an administrator uploaded for a collection, a blob. It wins over
-- the automatic one: TMDB's (`poster`), else its first film's.
ALTER TABLE collections ADD COLUMN custom_poster TEXT;

-- Only TMDB fills `poster` itself: any other collection's was set by hand,
-- through the Plex-compatible API, so it is the uploaded one.
UPDATE collections SET custom_poster = poster, poster = NULL WHERE tmdb_id IS NULL AND poster IS NOT NULL;
