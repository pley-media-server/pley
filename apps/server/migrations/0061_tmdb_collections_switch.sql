-- Whether a movie library gets collections from TMDB (a film's
-- belongs_to_collection): an administrator turns it off when another tool,
-- such as Kometa, makes the library's collections.
ALTER TABLE libraries ADD COLUMN tmdb_collections INTEGER NOT NULL DEFAULT 1;
