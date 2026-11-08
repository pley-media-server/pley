-- The country an item's content rating is from (ISO 3166-1, "GB"), so that
-- its age is read with that country's ratings: "A" is for all ages in
-- Denmark and for adults in India. NULL when it isn't known (a hand edit,
-- Kometa's bare numbers): the metadata region's ratings are used then (see
-- ratings.rs).
ALTER TABLE items ADD COLUMN content_rating_country TEXT;
