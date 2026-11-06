-- The lyrics file beside a music track (`<track>.lrc`, synced, or
-- `<track>.txt`), as the last scan read it. Its size and modification time
-- tell a changed file apart without reading it again; `text` is what it
-- says, at most 256 KiB. Lyrics in a track's own tags are read from its
-- probe data instead (see lyrics.rs).
CREATE TABLE track_lyrics (
    file_id INTEGER PRIMARY KEY REFERENCES media_files(id) ON DELETE CASCADE,
    path    TEXT    NOT NULL,
    size    INTEGER NOT NULL,
    mtime   INTEGER NOT NULL,
    text    TEXT    NOT NULL
);
