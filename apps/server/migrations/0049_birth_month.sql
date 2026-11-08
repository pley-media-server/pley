-- A profile whose age limit follows its age: the month and year it was
-- born ("2015-03"), never the day. users.max_age is kept equal to its age
-- (see accounts::birthday_ages), and is NULL from 21, the oldest age any
-- rating is for. Only administrators see it.
ALTER TABLE users ADD COLUMN birth_month TEXT;
