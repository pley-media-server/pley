-- Other, where collections without a category are listed, is one of Pley's
-- own categories, so administrators move it like the others and apps group
-- by what the server sends. It goes just before Franchises, the long list
-- TMDB fills. Collections without a category keep `category_id` NULL.
UPDATE collection_categories SET position = position + 1
    WHERE position >= (SELECT position FROM collection_categories WHERE key = 'franchises');
INSERT INTO collection_categories (key, position, created_at)
    VALUES ('other', COALESCE((SELECT position - 1 FROM collection_categories WHERE key = 'franchises'),
                              (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_categories)), 0);
