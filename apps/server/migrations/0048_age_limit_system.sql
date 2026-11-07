-- The rating system an age limit was chosen in and is shown in: a country
-- with ratings in ratings.rs ("GB"), or "age" for a plain age. NULL: the
-- metadata region's. The limit itself stays users.max_age (and
-- invites.max_age), which is what's enforced.
ALTER TABLE users ADD COLUMN max_age_system TEXT;
ALTER TABLE invites ADD COLUMN max_age_system TEXT;
