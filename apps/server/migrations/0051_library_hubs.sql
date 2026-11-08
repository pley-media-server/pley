-- The rows of a library, in order (see hub_layout.rs): its built-in rows
-- (`hub` is 'continue_watching', 'recently_added', 'recently_released' or
-- 'top_rated') and the collections promoted to rows ('collection:{id}',
-- with collection_id set, so the row goes with the collection). Each says
-- where it shows: the library's own page, administrators' Home, everyone
-- else's Home. A library with no rows here shows its built-in rows as Pley
-- always has.
CREATE TABLE library_hubs (
    library_id    INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    hub           TEXT    NOT NULL,
    collection_id INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    on_library    INTEGER NOT NULL,
    on_admin_home INTEGER NOT NULL,
    on_home       INTEGER NOT NULL,
    PRIMARY KEY (library_id, hub)
) WITHOUT ROWID;
CREATE INDEX library_hubs_collection ON library_hubs(collection_id) WHERE collection_id IS NOT NULL;
