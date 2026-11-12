-- Whether a cast put an app on the profile it acts as (a switch with the
-- cast's ticket), until the profile is picked on the device itself. The
-- caster gave their PIN on their own device, not this one: an app a cast
-- switched to an administrator's profile plays as it, without the role's
-- rights over others (AuthUser::role_holds).
ALTER TABLE app_grants ADD COLUMN by_cast INTEGER NOT NULL DEFAULT 0;
