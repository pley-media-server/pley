-- Requests for movies and shows the libraries don't have (src/requests):
-- the title as TMDB had it when asked, the seasons asked for (shows: a JSON
-- array of TMDB's season numbers, 0 for specials), and what became of it.
-- A request outlives its library title (`item_id` is set NULL when the
-- title goes) and the administrator who decided it, not its maker.
-- `arr_id`: Radarr's movie or Sonarr's series, once added there.
-- Times are unix seconds.
CREATE TABLE requests (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    kind              TEXT    NOT NULL CHECK (kind IN ('movie', 'show')),
    tmdb_id           INTEGER NOT NULL,
    tvdb_id           INTEGER,
    imdb_id           TEXT,
    title             TEXT    NOT NULL,
    year              INTEGER,
    poster_path       TEXT,                    -- TMDB's: '/abc.jpg'
    backdrop_path     TEXT,
    seasons           TEXT,                    -- shows: '[1,2]'; NULL for movies
    available_seasons TEXT    NOT NULL DEFAULT '[]',
    anime             INTEGER NOT NULL DEFAULT 0,
    user_id           INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status            TEXT    NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'declined', 'failed', 'available')),
    reason            TEXT,
    decided_by        INTEGER REFERENCES users(id) ON DELETE SET NULL,
    decided_at        INTEGER,
    item_id           INTEGER REFERENCES items(id) ON DELETE SET NULL,
    arr_id            INTEGER,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    available_at      INTEGER
);
CREATE INDEX requests_user   ON requests(user_id, created_at);
CREATE INDEX requests_status ON requests(status, created_at);
CREATE INDEX requests_title  ON requests(kind, tmdb_id);
CREATE INDEX requests_item   ON requests(item_id) WHERE item_id IS NOT NULL;
CREATE INDEX requests_decider ON requests(decided_by) WHERE decided_by IS NOT NULL;

-- Library titles found by their ids: what Discover shows as in a library,
-- and requests that became available.
CREATE INDEX items_tmdb ON items(tmdb_id) WHERE tmdb_id IS NOT NULL;
CREATE INDEX items_tvdb ON items(tvdb_id) WHERE tvdb_id IS NOT NULL;
CREATE INDEX items_imdb ON items(imdb_id) WHERE imdb_id IS NOT NULL;

-- Who may ask: 'off' may not, 'ask' asks an administrator, 'auto' is
-- approved as asked (administrators always are). Profiles without a
-- password and accounts with an age limit may not until an administrator
-- says so: Discover shows TMDB's titles whatever their rating.
ALTER TABLE users ADD COLUMN request_access TEXT NOT NULL DEFAULT 'ask'
    CHECK (request_access IN ('off', 'ask', 'auto'));
UPDATE users SET request_access = 'off' WHERE managed = 1 OR max_age IS NOT NULL;
