-- Requests, second part (src/requests): accounts follow requests others
-- made, and a request keeps where it went, how far it is, and when it last
-- had news for the people waiting for it. Times are unix seconds.

-- Who follows a request besides its maker: asking for what someone asked
-- for already follows their request instead of making another.
CREATE TABLE request_watchers (
    request_id INTEGER NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (request_id, user_id)
);
CREATE INDEX request_watchers_user ON request_watchers(user_id);

-- `sent_to`: where an approved request went, Radarr or Sonarr (`arr_id`
-- there once added), or 'hand' when a manager added it by hand: nothing is
-- sent for those, ever. `sent_profile` (the quality profile's name) and
-- `sent_folder`: what it was sent with. `sent_seasons`: the seasons
-- monitored in Sonarr, as a JSON array, where Sonarr numbers them unlike
-- TMDB (NULL: those asked for).
-- `attempts` and `retry_at`: sends that failed, and when the server tries
-- again by itself (NULL: it won't; a manager decides).
-- `stage`, `stage_at` and `note`: how far it is, as last known, written
-- whenever they change, so answers after a restart show the last ones.
-- `news_at`: when it last became available, partly available or declined;
-- that is news to its maker and followers who looked before.
-- `inherited_at`: when its earliest follower became its maker, its own
-- maker having withdrawn it or left; it doesn't count toward their limits.
ALTER TABLE requests ADD COLUMN sent_to TEXT CHECK (sent_to IN ('radarr', 'sonarr', 'hand'));
ALTER TABLE requests ADD COLUMN sent_profile TEXT;
ALTER TABLE requests ADD COLUMN sent_folder TEXT;
ALTER TABLE requests ADD COLUMN sent_seasons TEXT;
ALTER TABLE requests ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE requests ADD COLUMN retry_at INTEGER;
ALTER TABLE requests ADD COLUMN stage TEXT;
ALTER TABLE requests ADD COLUMN stage_at INTEGER;
ALTER TABLE requests ADD COLUMN note TEXT;
ALTER TABLE requests ADD COLUMN news_at INTEGER;
ALTER TABLE requests ADD COLUMN inherited_at INTEGER;
CREATE INDEX requests_retry ON requests(retry_at) WHERE retry_at IS NOT NULL;

-- Those sent already; approved ones that weren't count as added by hand,
-- so connecting Radarr or Sonarr later sends nothing by surprise.
UPDATE requests SET sent_to = CASE kind WHEN 'movie' THEN 'radarr' ELSE 'sonarr' END WHERE arr_id IS NOT NULL;
UPDATE requests SET sent_to = 'hand' WHERE arr_id IS NULL AND status = 'approved';
UPDATE requests SET news_at = COALESCE(available_at, updated_at) WHERE status = 'available';
UPDATE requests SET news_at = COALESCE(decided_at, updated_at) WHERE status = 'declined';
UPDATE requests SET news_at = updated_at WHERE status = 'approved' AND available_seasons != '[]';

-- When each account last looked at its requests (POST /api/requests/seen):
-- what had news before then is seen. Accounts there already have seen
-- everything so far; new ones nothing yet.
ALTER TABLE users ADD COLUMN requests_seen_at INTEGER;
UPDATE users SET requests_seen_at = CAST(strftime('%s', 'now') AS INTEGER);
