-- Movies of a library matched to the same TMDB entry are one movie, with
-- each copy a version (see `library::merge`). The keys the merged movies'
-- files made (`movie:interstellar:2014`) are kept here, so that a later
-- scan of those files finds the movie they went into instead of making
-- them a movie of their own again. Fixing the movie's match lets them go.
CREATE TABLE item_aliases (
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    key        TEXT    NOT NULL,
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    PRIMARY KEY (library_id, key)
) WITHOUT ROWID;
CREATE INDEX item_aliases_item ON item_aliases(item_id);

-- Copies that fixing a merged movie's match let go: the movie made by the
-- key `key` stays apart from movie `item_id`, even when matched to the same
-- title again, as an automatic match that merged them wrongly would be.
-- Fixing either movie's match by hand forgets it.
CREATE TABLE movies_apart (
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    key        TEXT    NOT NULL,
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    PRIMARY KEY (library_id, key, item_id)
) WITHOUT ROWID;
CREATE INDEX movies_apart_item ON movies_apart(item_id);
