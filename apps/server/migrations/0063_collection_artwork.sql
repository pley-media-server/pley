-- A poster an administrator uploaded for a collection, a blob. It wins over
-- the automatic one: TMDB's (`poster`), else its first film's.
ALTER TABLE collections ADD COLUMN custom_poster TEXT;

-- Only TMDB fills `poster` itself: any other collection's was set by hand,
-- through the Plex-compatible API, so it is the uploaded one.
UPDATE collections SET custom_poster = poster, poster = NULL WHERE tmdb_id IS NULL AND poster IS NOT NULL;

-- A collection's poster can be an icon in a color, drawn over its first
-- films' posters (`icon` and `icon_color` are names from
-- `src/collection_art.rs`). It wins over TMDB's poster; an uploaded one
-- (`custom_poster`) and an icon are never both set.
ALTER TABLE collections ADD COLUMN icon TEXT;
ALTER TABLE collections ADD COLUMN icon_color TEXT;

-- The collections Pley makes start with an icon each.
UPDATE collections SET
    icon = CASE builtin
        WHEN 'recently_added' THEN 'clock'
        WHEN 'recently_released' THEN 'calendar'
        WHEN 'top_rated' THEN 'star'
        WHEN 'top_unwatched' THEN 'eye-off'
    END,
    icon_color = CASE builtin
        WHEN 'recently_added' THEN 'blue'
        WHEN 'recently_released' THEN 'teal'
        WHEN 'top_rated' THEN 'amber'
        WHEN 'top_unwatched' THEN 'pink'
    END
WHERE builtin IN ('recently_added', 'recently_released', 'top_rated', 'top_unwatched') AND custom_poster IS NULL;

-- A collection's icon can be drawn on its color alone instead of over its
-- films' posters: `color`, or NULL for the posters. Only with an icon.
ALTER TABLE collections ADD COLUMN poster_background TEXT;
