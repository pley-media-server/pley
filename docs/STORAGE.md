# Storage

What Pley keeps on disk, how to back it up, restore or move it, and how it
is cleaned up. Everything lives in the data folder (`--data-dir`, see
[CONFIGURATION.md](CONFIGURATION.md#options)). Your media and the artwork next to it
are only read, never moved or changed.

## The data folder

```
<data>/
  pley.toml             the config file (unless --config names another)
  pley.db               the database (SQLite, WAL: also pley.db-wal, -shm)
  pley.lock             held while Pley runs: one server per data folder
  secret.key            encrypts authenticator secrets, keys network hashes
  plex-machine-id       the compatibility API's server id
  metadata/blobs/       pictures that are metadata, named by their SHA-256
  media/ab/<fingerprint>/
    thumb.jpg           frame grab (movies and episodes without a still)
    trickplay/          seek-bar previews (sprite sheets, index-sd.bif)
    chapters/<n>.jpg    chapter pictures, cut from the previews
  subtitles/<file_id>/  subtitles people added, as UTF-8 .srt/.vtt/.ass/.ssa
  avatars/              profile pictures
  backups/              database backups
  cache/                safe to delete; old files are removed (see Clean-up)
  transcode/            the managed transcoder's work folder
  logs/pley.log         the server log, then pley.log.1 to .3 (10 MB each)
```

- **Pictures** (downloaded artwork, uploads, embedded album covers, people's
  photos) are stored once however many items use them. The database refers
  to them as `blob:<sha256>.<ext>`, never by absolute path, so you can move
  or remount the data folder (Docker). Downloads remember their URL, so a
  refresh that finds the same URL reuses the stored copy. Local artwork
  (`poster.jpg`, `fanart.jpg`, `cover.jpg`, …) is referenced by its path and
  always wins.
- **Generated files** are named by the media file's fingerprint (its
  OpenSubtitles hash: size plus a checksum of its first and last 64 KiB), so
  they follow a file that is moved or renamed. A changed file gets a new one.
- **`transcode/`** holds stream segments, the transcoder's caches and
  converted downloads; the transcoder prunes it
  ([TRANSCODER.md](TRANSCODER.md#work-folder)). Settings → Transcoder,
  `--transcode-dir` or `transcode_dir` in `pley.toml` move it elsewhere.
- Pley never writes next to your media.

## Backups

- **Daily**: once a day, and at start when the last backup is over a day
  old, Pley writes `backups/pley-YYYY-MM-DD-HHMM.db` with SQLite's
  `VACUUM INTO` while it runs. Each copy is checked; the last three are kept.
- **Before an upgrade**: before a new version changes the database schema,
  Pley writes a checked `pley-YYYY-MM-DD-HHMM-before-upgrade.db`, and doesn't
  start if it can't (a full disk, say). It is kept until the next upgrade.
- **On demand**: `POST /api/system/backups`. `GET /api/system/backups` lists
  them, with the result of the database check that runs at every start.

Never copy `pley.db` while Pley runs: recent changes live in `pley.db-wal`,
so the copy would be stale or inconsistent. Use a backup.

**To restore one**: stop Pley, move `pley.db`, `pley.db-wal` and
`pley.db-shm` aside, copy the backup to `pley.db`, and start Pley.

**To move Pley to another computer**: stop it and copy the whole data
folder. Without `secret.key`, people with two-step sign-in must use a
recovery code and set it up again (or an administrator turns it off for
them), and Automatic quality forgets the networks it measured.

## Clean-up

Runs after scans that removed items, once a day, and on demand
(`POST /api/system/storage/clean`). `GET /api/system/storage` reports the
space used.

- Pictures and media folders nothing uses any more (a removed movie, a
  replaced poster) are removed.
- Nothing younger than an hour is removed, nor anything handed out in the
  last hour, however old. This is checked right before each removal.
- Added subtitles go once their media file leaves its library.
- Cache files go at the age below. Their names come from their source (path,
  size, modification time), so stale ones are never asked for again, and a
  file needed later is made again.

| Folder | What | Kept for |
|---|---|---|
| `cache/images` | resized pictures | 30 days |
| `cache/covers` | albums known to have no embedded cover | 30 days |
| `cache/subtitles` | subtitles converted to WebVTT, picture subtitles copied into Matroska (`.mks`) | 90 days |
| `cache/fingerprints` | audio fingerprints (intro and credits detection) | 90 days |
| `cache/live/pictures` | Live TV channel logos and programme pictures | fetched again after 7 days; removed after 30 days unused, or oldest first past 256 MB |
| `cache/live/downloads` | guide files being read | emptied at every start |
| `cache/discover` | TMDB's pictures for Discover and requests, as `<size>/<file>` (TMDB's size and file name), with no database row | 30 days, or oldest first past 256 MB |
