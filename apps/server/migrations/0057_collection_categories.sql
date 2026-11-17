-- Collections are grouped by category, and administrators choose which
-- collections are rows on Home and on their library's Recommended tab, in
-- what order. What were built-in rows (Recently Added, Recently Released,
-- Top Rated) become collections Pley makes in each movie and TV library,
-- which administrators can change like any other.

-- Built-in categories have a `key` and are named in each language by the
-- server; an administrator's own have a `title`. A category made from a
-- collection's group label keeps that label, so it is still found after
-- it's renamed.
CREATE TABLE collection_categories (
    id         INTEGER PRIMARY KEY,
    key        TEXT UNIQUE,
    title      TEXT,
    from_label TEXT,
    position   INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    CHECK ((key IS NULL) <> (title IS NULL))
);
CREATE UNIQUE INDEX collection_categories_title
    ON collection_categories(title COLLATE NOCASE) WHERE title IS NOT NULL;
CREATE UNIQUE INDEX collection_categories_label
    ON collection_categories(from_label COLLATE NOCASE) WHERE from_label IS NOT NULL;

INSERT INTO collection_categories (key, position, created_at) VALUES ('highlights', 0, 0), ('franchises', 1, 0);

-- Deleting a category leaves its collections without one.
ALTER TABLE collections ADD COLUMN category_id INTEGER REFERENCES collection_categories(id) ON DELETE SET NULL;
-- Who chose the category: Pley (a built-in one, when it made the
-- collection), a group label, or an administrator. Labels change only a
-- category that is unset or came from a label.
ALTER TABLE collections ADD COLUMN category_from TEXT CHECK (category_from IN ('pley', 'label', 'admin'));
-- Where the collection is a row, in what order: Home (shared by every
-- collection of the same title, which show as one row) and its library's
-- Recommended tab. NULL: not a row there.
ALTER TABLE collections ADD COLUMN home_position INTEGER;
ALTER TABLE collections ADD COLUMN recommended_position INTEGER;
-- One of the collections Pley makes in each library: recently_added,
-- recently_released, top_rated, top_unwatched. Kept with its title (named
-- in each language until an administrator renames it) and to restore it.
ALTER TABLE collections ADD COLUMN builtin TEXT;
CREATE UNIQUE INDEX collections_builtin ON collections(library_id, builtin) WHERE builtin IS NOT NULL;

-- Music and audiobook libraries have no collections: their Recently Added
-- stays a built-in row, placed on Home by this.
ALTER TABLE libraries ADD COLUMN home_position INTEGER;

UPDATE collections SET category_id = (SELECT id FROM collection_categories WHERE key = 'franchises'),
    category_from = 'pley'
WHERE tmdb_id IS NOT NULL;

-- Pley's collections for every movie and TV library, and the Home row of
-- every library's Recently Added, in the libraries' order as Home had them.
CREATE TEMP TABLE library_rows AS
SELECT id, kind, ROW_NUMBER() OVER (ORDER BY position, name COLLATE NOCASE, id) - 1 AS row
FROM libraries;

INSERT INTO collections (library_id, title, sort_title, sort, smart_filter, builtin, category_id, category_from,
                         home_position, recommended_position, created_at, updated_at)
SELECT l.id,
       CASE WHEN d.builtin = 'recently_added' AND l.kind = 'shows' THEN 'Recently Added Shows' ELSE d.title END,
       CASE WHEN d.builtin = 'recently_added' AND l.kind = 'shows' THEN 'recently added shows' ELSE d.sort_title END,
       'release',
       CASE WHEN d.builtin = 'recently_added' AND l.kind = 'shows' THEN 'sort=episode.addedAt:desc,id:desc&limit=100'
            ELSE d.filter END,
       d.builtin, (SELECT id FROM collection_categories WHERE key = 'highlights'), 'pley',
       CASE WHEN d.builtin = 'recently_added' THEN l.row END, d.position, 0, 0
FROM library_rows l
JOIN (SELECT 'recently_added' AS builtin, 'Recently Added' AS title, 'recently added' AS sort_title,
             'sort=addedAt:desc,id:desc&limit=100' AS filter, 0 AS position
      UNION ALL SELECT 'recently_released', 'Recently Released', 'recently released',
             'originallyAvailableAt!=-1&sort=originallyAvailableAt:desc&limit=100', 1
      UNION ALL SELECT 'top_rated', 'Top Rated', 'top rated', 'rating!=-1&sort=rating:desc,titleSort&limit=100', 2
      UNION ALL SELECT 'top_unwatched', 'Top Unwatched', 'top unwatched',
             'unwatched=1&rating!=-1&sort=rating:desc,titleSort&limit=100', 3) d
WHERE l.kind IN ('movies', 'shows');

UPDATE libraries SET home_position = (SELECT row FROM library_rows WHERE library_rows.id = libraries.id)
WHERE kind NOT IN ('movies', 'shows');

DROP TABLE library_rows;
