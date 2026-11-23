-- A Live TV source whose channels play on another source with it too, which
-- has the same channel, when its own connections are all in use.
ALTER TABLE live_sources ADD COLUMN failover INTEGER NOT NULL DEFAULT 0;
