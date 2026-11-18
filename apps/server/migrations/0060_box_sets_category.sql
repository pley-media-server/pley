-- TMDB's collections are runs of films, such as a trilogy, rather than whole
-- franchises: Pley's category for them is Box sets.
UPDATE collection_categories SET key = 'box_sets' WHERE key = 'franchises';
