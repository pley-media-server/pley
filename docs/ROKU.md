# Pley for Roku

A native SceneGraph app for Roku players and Roku TVs, in `apps/roku`
(paths below are relative to it). It is not in Roku's channel store: you
sideload it in developer mode.
It talks only to the Pley server, so it works without internet: pictures
come from the server (Discover's from TMDB through it). Discover needs the
server to reach TMDB.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

1. Turn on developer mode: on the Roku remote press Home three times, Up
   twice, then Right, Left, Right, Left, Right. Note the address shown,
   choose a password and let the Roku restart.
2. Get the zip: the one attached to a Roku release
   (`pley-roku-<version>.zip`), or `bun run package`, which writes
   `out/pley-roku.zip`.
3. Open the Roku's address in a browser, sign in as `rokudev` with that
   password, upload the zip and press Install. Pley appears on the Home
   screen; installing again replaces it.

`bunx bsc --deploy --host <address> --password <password>` packages and
installs in one go. The debug console (`telnet <address> 8085`) shows what
the app prints and its errors.

## Using it

The app finds Pley servers on the local network (or takes an address) and
signs in with a code shown on the TV, approved on the server's `/link` page
or by its QR code. When the server requires two-step sign-in, the sign-in
screen says so: the phone that approves the code sets it up for whoever
hasn't yet. It asks "Who's watching?" at start when the household has more
than one profile, unless Settings' Always start as names one.

### Remote keys

| Key | Where | What it does |
|---|---|---|
| Options (\*) | A title | Mark watched, My List, Add to playlist, Remove from Continue Watching, Go to show, Go to season |
| Options (\*) | Watch history | Removes it |
| Options (\*) | A playlist entry | Move (Up and Down carry it, OK puts it there, Back puts it back), Remove from playlist |
| Options (\*) | A request | What can be done with it (Approve, Decline, Withdraw…) |
| Down | The player | Pley's menu: Quality, Subtitle sync, Chapters, Sleep timer, Find subtitles…, Playback details |
| Options (\*) | The player | Roku's own options: subtitles and audio tracks |
| OK | Skip intro, Skip credits | Skips; fast forward and rewind move 10 s |
| Replay | Live TV | Search (Roku remotes have no search key for apps) |

### A title's page

A movie's, an episode's or a show's page has its buttons in a column at
the left, Play (or Resume and Play from start) first, then Mark as
watched, Audio and subtitles, My List, Trailer (where its extras have
one: it plays from its start) and More…. Audio and subtitles and More
open a panel at the right; Back closes it, and Down from the buttons goes
to the rows under them.

- **Audio and subtitles** (a movie or an episode with several files, or
  whose file has two audio tracks or more, or subtitles):
  - Version, for several files: one line each ("Director's Cut · 4K ·
    HEVC · DV P8.1 · TrueHD Atmos 7.1 · 62.0 GB"). Play plays the one
    chosen (`file_id` in `POST /api/playback`), with the tracks the server
    picks; it isn't saved, as on the other apps.
  - Audio, Automatic first, named by the language the profile's defaults
    pick ("Automatic (English)"), then the file's tracks ("French (AC3
    5.1)"); Subtitles, Off first, then the file's ("English – SDH (SRT)").
    A choice is saved at once for the show or movie (`PUT
    /api/items/{id}/streams`) and Play starts with it. The audio goes with
    it only when picked here, Automatic as none, as on the other TVs.
- **More**, what is used less, as on the other TVs: Rate (Your rating,
  once given: No rating, or 1 to 5 stars, kept for the profile on the
  server), an episode's Go to show (the show's page) and Go to season
  (its episodes), Media info and Add to playlist. The Options key on an
  episode's tile goes to them alike.
- **Media info** (a movie or an episode with a file): each file, under its
  edition or "VERSION 2" when there are several, and every stream in
  full, in the other apps' words (codec, profile and level, size, frame
  rate, bit depth, HDR and Dolby Vision, colour, channels, sample rate,
  language, title, flags), as text that Up and Down scroll.

### Collections, people, artists and playlists

A collection's page has its backdrop, "Collection" with a TMDB or Smart
badge, its title, what it holds ("5 movies · Movies · By release date";
a smart collection's filter in words), its summary, Play and Shuffle (two
titles or more), then its posters, which Down slides the page up to. Play
plays a collection of movies in its order, Up Next bringing on each next,
and Shuffle in any order; a collection of shows plays the first show's
next episode, and Shuffle a random show's.

A person's page has their photo, their name and what of theirs is in the
libraries ("3 movies · 2 shows in your libraries"), then a row for each
thing they did (Acting, Directing, Creating, Writing, Producing) of their
titles, newest first, each with its year and the part ("2019 · as Neo").

An artist's page has their picture, their name, albums, tracks and
genres, their summary, and Play and Shuffle, which hand every track of
theirs to the music player, album by album, and open Now Playing (Now
playing goes back to it while music plays); their albums are on the
right, newest first.

Playlists has New playlist at the right of its heading: what it is for
(Movies & TV or Music), then its name on the Roku's keyboard. The new
playlist opens, empty; Add to playlist, the Options key on a title, fills
it. A playlist's entries move with the Options key's Move.

### Libraries

A library has the tabs Recommended, Library and Collections (a music
library has no Collections), and opens on the tab, sort and filters it was
left on, kept on the Roku for each server, profile and library.

- **Library** is a grid of posters, 70 at a time as the focus comes near
  the next ones; places a jump passes over show empty frames until their
  titles come.
- **See all** ends the rows of a library's newest, newly released and top
  rated titles, on Home and on a library's Recommended tab. It shows the
  library's Library tab in the row's order (date added, release date or
  rating, newest or best first), nothing filtered, for that visit: the
  sort and filters kept stay as they were until changed there.
- **A to Z:** sorted by title with more than 30 titles, "#" and A to Z
  stand in a column right of the grid, the letter of the title in focus
  marked and the letters no title starts with dimmed. Right from the
  grid's last column reaches it, Up and Down pick a letter, OK goes to its
  first title (or the next letter's) and Left goes back. The server has no
  index of letters: the app reads the listing's titles a thousand at a
  time and keeps only where each letter starts
  (`components/lib/Library.brs`).
- **Save as…:** once filters are set, the Filter panel ends with Clear
  filters and Save as…, which saves them and the order as a smart playlist
  (`POST /api/playlists` with `library_id` and `smart_filter`, written as
  the other apps write it), named on the Roku's keyboard, and opens it.

### Search

Search is typed on the Roku's keyboard and asked once typing rests half a
second. What the libraries have comes in rows by kind, in the other apps'
order: People, Live TV (cards: OK watches the channel, and Up and Down in
the player go through what was found), Movies, Shows, Seasons, Episodes,
Artists, Albums and Tracks; then On TMDB for those who may ask (below).
Right from the keyboard's last column reaches them.

Before anything is typed it lists the recent searches: the last 10 whose
results were opened, newest first, kept on the Roku for each server and
profile. OK searches one again at once and moves it to the top; Clear
forgets them.

### Music

Music plays on while other screens show, and Home's menu has Now Playing
while it does. An album's page lists its tracks under "Disc 1", "Disc 2"
when they are on several discs (the tracks' `parent_index`), each
numbered on its disc; the focus passes over the headings. An album or a
music playlist plays whole, from the track chosen, so Previous goes back
past it; its Shuffle starts on any track and plays the others after it in
a random order.

Now Playing shows the track, its cover and how far along it is, Shuffle,
Previous, Pause, Next, Stop and the sleep timer, and Up next: the tracks
after the one playing, numbered by their place in the queue. OK on one
plays it, and the queue goes on from there. Shuffle plays the other tracks
after the one playing in a random order; Stop shuffling puts them back in
the album's or playlist's order, at the same track. Play pauses or goes
on; fast forward and rewind go to the next track or back.

The sleep timer belongs to the music, not to Now Playing: it runs while
other screens show. Each OK on its button steps Off, 15, 30, 45, 60 and 90
min, End of this track, and the button says what is left ("Sleep timer:
23 min left"). In its last 30 s Now Playing says "Pausing in 12 s" and the
button is Keep listening, which adds the same time again (or lets this
track play to its end). Then the music pauses until Play, as on the other
apps; at the end of a track, the next one waits, paused. Stop, or the
queue's end, turns the timer off.

### The player

Fast forward and rewind show seek previews in Roku's own bar: the server's
previews of the file as a BIF file, which the Video node fetches beside the
media (`previews.bif`). While the server is still making them, the bar
shows the time alone.

The sleep timer (Down, then Sleep timer) pauses after 15 to 90 minutes
or at the end of what plays, as on the other apps. In its last
30 s it says "Pausing in 12 s" with Keep watching, which adds the same time
again (or lets this title play to its end). Paused by it for 10 minutes,
the player leaves; it isn't kept from one time the player opens to the
next.

### Audio and subtitles

- Roku shows a subtitle track only with captions on, a Roku-wide setting.
  When Pley picks a track it turns captions on for that playback and
  restores them after.
- Subtitles look as the Roku's caption settings say (Settings →
  Accessibility → Captions style). The app never sets `captionStyle`,
  which would change them for the whole Roku.
- Text subtitles go to the Video node as WebVTT. Picture subtitles (PGS,
  VobSub, DVB) are burned in by the server.
- **Subtitle sync:** Left and Right move subtitles 0.1 s, up to 10 s either
  way. The server shifts the cues (`offset_ms`) and the stream reopens
  where it was. The offset is kept per file per server (the latest 50).
- **Find subtitles…** searches OpenSubtitles in the profile's subtitle
  language, or another that Language, above the results, picks
  (`components/lib/SubtitleSearch.brs`). OK on a result adds it on the
  server and shows it.
- The server picks the audio this Roku plays best in the profile's
  language. Any audio but the file's default makes the file a stream with
  its video copied.
- Picks in Roku's options are saved for the show or movie, so the next
  episode starts with the same tracks. Roku's audio tracks are matched to
  the file's only when their count and languages agree.

### Quality

The menu, Original and Automatic follow [QUALITY.md](QUALITY.md#the-menu);
the code is `components/lib/AutoQuality.brs`. On Roku:

- **Settings → Quality** is kept on the Roku. One chosen in the player's
  menu becomes the setting, and the title reopens at it.
- **Several sizes:** in an adaptive session the Video node moves between
  sizes itself. Other streams come in one size, so the app steps down.
- **Signals:** a wait is the Video node's `buffering` after at least 2 s of
  playing, and only when `bufferingStatus.isUnderrun` says it is for want
  of data. Network speed comes from the HLS segments' sizes and download
  times. Roku reports neither read-ahead nor dropped frames.
- Roku follows the rules for
  [players that go black between streams](QUALITY.md#players-that-go-black-between-streams):
  a conversion whose bitrate can change is lowered in place; the original,
  or a fixed-bitrate conversion, moves to a new session at least one
  quality lower after a wait of 2 s or more.
- **Memory:** each step down is kept 30 minutes per server and told to the
  server. When the server asks for a speed test, the app times 8 MiB of
  `GET /api/playback/speedtest`, giving up after about 3 s.

### Live TV

Live TV works as in the [Android app](ANDROID.md#live-tv). On Roku
(`components/screens/LiveTvScreen`, `LivePlayerScreen`,
`components/lib/Live.brs`):

- **Paging:** channels come 50 at a time, and the guide only for those 50
  (`GET /api/live/guide?ids=`).
- **Categories** run down the left under Search. Rewind and fast forward
  move a screen at a time. The category and the tab are kept per server
  and profile.
- **Guide:** Roku's TimeGrid, 50 channels by two hours. Rewind and fast
  forward move two hours, Play goes back to now. OK on what's on watches
  the channel; on a later programme it shows its details.
- **Channels:** the other tab, as cards with what's on and what's next.
- **The player** tunes with `POST …/watch`, waits for the server's live HLS
  playlist (up to 50 s), then plays it with `Live = true`. Up and Down
  change channel, tuning 0.7 s after the last press. Left opens the
  channel list; Right, OK or Options show the banner. Leaving, or changing
  channel, sends `DELETE /api/live/sessions/{session}`.
- **Radio:** a channel with no picture keeps its card with what's on.

### Requests and Discover

Where an administrator has turned requests on, people ask for movies and
shows found on TMDB, administrators approve them, and Radarr and Sonarr
fetch them ([API.md](API.md#requests)). An administrator's Roku manages
them (a manager, below) once its sign-in allows `approvals`; one signed in
before is offered Sign in again. The menu shows what the profile may do;
its number counts what waits: a manager's pending and failed requests
(red while some failed) and their own news, anyone else's news.

- **Discover**, for those who may ask: a hero for the first trending title
  (its main action and Details; once nothing of its kind is left to ask
  for, the limit's line in place of Request), then rows of posters with
  their badges ("In library", "Partly in library", "Requested",
  "Declined"): Trending this week, Popular movies, Popular shows, Coming to
  cinemas, Popular anime, each loaded as it comes into view and ending with
  See all (a grid, a page at a time; after five pages in a row that leave
  it short of a screenful, as an age limit can, Look further asks for
  more); then Your requests, from the first one asked for. Search at the
  top looks on TMDB, a page at a time likewise.
- **A TMDB title's page:** backdrop and poster, year, runtime or seasons,
  certification, genres, rating, tagline and overview; one main action
  (Open, Request, Request more seasons, Tell me too) or the line that says
  why there is none (the limit and when it frees up, declined, not out
  yet); its request's stage and what can be done with it; rows of seasons,
  each saying how it stands, and the cast. A movie's Request asks first,
  saying what will happen; a show's opens **Which seasons?**, those left to
  ask for ticked, OK ticking or unticking one, then Request 3 seasons.
- **Your requests** (a manager's **Requests**): tabs by status. A
  manager's open on Needs attention (pending, and failed ones the server
  won't retry, oldest first), Yours holds their own, and Filter keeps
  movies or shows, one person, or titles with some words (the Roku's
  keyboard). A card per request: poster, seasons, a stage pill (amber
  waiting for approval; blue approved, searching, or downloading with a
  bar and the time left; green in library; red declined or failed; gray
  gone), who asked and when, the reason or note. OK opens the title; the
  Options key (\*) offers Watch, Withdraw, Remove, Stop following or
  Request again, and to managers Approve, Try again, Decline, Decline with
  a reason… (the keyboard), Mark as added by hand and Delete; Withdraw,
  Remove and Delete ask first. Opening the account's own requests marks
  their news seen.
- **Search** shows titles **On TMDB** after the libraries' own, for those
  who may ask. A library title that was asked for shows its request's
  stage under the poster, and a show with seasons left to ask for ends its
  seasons with Request more seasons.
- **Ready to watch:** a requested title that arrives is told of ("“The
  Last Lighthouse” is ready to watch." with Open and OK; "3 of your
  requests have news" for several), never over playing video, a sign-in or
  another dialog: it waits for them, and shows again after a dialog that
  takes its place.

Not offered: Approve's choices of profile, folder and seasons (on the web
and phones; Approve here approves what was asked), bulk approve and
decline, and the requests' settings (the web).

### Settings

Settings has the audio and subtitle defaults, Quality, Play the next
episode by itself (Up Next counts down only while it is on), Watch
history, Switch profile, Always start as, Sort libraries, Switch server
and Sign out. It says where the account is managed on the web (the
server's address and `/account`), and carries TMDB's notice. Right of the
settings, Diagnostics shows this Roku and what it tells the server it
plays (`capsLines` in `components/lib/Caps.brs`), and Open-source licences
shows Pley's licence and the notices of what the package bundles.

Always start as, for a household of several profiles, names the profile
the app starts as, or Ask who's watching. As on the other TVs it only
skips "Who's watching?" at a start: the app starts as the profile it was
left on. It is kept on the Roku for each server.

## Building and testing

### How it works

| Path | What |
|---|---|
| `source/main.brs` | Starts the scene |
| `components/MainScene.*` | The screen stack, the HTTP agent for pictures, sign-in when the server ends it |
| `components/MusicPlayer.*` | The music queue on the scene's Audio node |
| `components/screens/` | One component per screen |
| `components/shared/` | Screen helpers (`Ui.brs`), title actions (`Titles.brs`) |
| `components/items/` | Tiles, rows, the player's skip button |
| `components/tasks/ApiTask` | The only code that talks to the server |
| `components/tasks/DiscoveryTask` | `pley?` to UDP port 7539 (`apps/server/src/discovery.rs`) |
| `components/tasks/EventsTask` | The server's event stream, on a socket |
| `components/lib/` | Plain BrightScript (no SceneGraph): the logic the unit tests cover |
| `tests/` | Unit tests of `components/lib` |
| `scripts/` | Test runner, smoke run, preview, bench, images |
| `licenses/` | Notices for bundled third-party code (BrighterScript's `bslib.brs`) |

Rules the code keeps:

- All HTTP goes through `ApiTask`; the render thread never makes a request.
- The access token goes only in `Authorization: Bearer`, never in an
  address, including the Video node's and the pictures' HTTP agents.
- Videos play from [keyed addresses](API.md#keyed-urls): Roku fetches a
  BIF file of seek previews without its HTTP agent's headers, so the
  previews come from the session's key, beside its media.
- Refreshes run one at a time per server, and the new token pair is saved
  before use (a refresh token works once).
- The User-Agent is `pleyRoku/<version> (Roku OS <version>; <model>)`; the
  server's `/link` page shows it.
- The event stream (`GET /api/events`) works with `http://` servers only:
  Roku's sockets have no TLS.
- Settings and views are kept in registry sections: `playback`,
  `subtitleSync`, `libraryViews`, `liveViews`, `household`, `startAs`,
  `requests`, `searches`.
- Sign-in asks for `library play playlists profiles`, and `requests
  approvals` when the server's metadata lists them in `scopes_supported`
  (an older server refuses scopes it doesn't know). A sign-in from before
  requests learns it from the summary (`sign_in_again`), on Discover and
  the requests.
- **Requests' summary** (`GET /api/requests/summary`;
  `components/lib/Requests.brs`) decides the menu, its number and what the
  screens offer. It is asked with each profile, when the event stream
  connects, a second after request events (once for those that come
  together), after the person acts on a request, and each minute without
  the event stream. An older server's `404`, or a `403` from one built
  before apps could ask, turns requests off until the stream connects
  again or the app starts.
- **Request changes:** request events give every tile of their titles, on
  covered screens too, its new badge: the title's status for the profile
  (`GET /api/discover/{kind}/{tmdb_id}`, once a title for events that come
  together), never one request's. The screen on top that shows requests
  asks again; nothing reloads whole.
  Without the stream a TMDB title's page showing a request asks each 30 s;
  while something downloads, title pages (TMDB's and the libraries'), the
  requests and Discover's Your requests ask each 10 s.
- **Notices** are kept per server and profile (registry section
  `requests`, the newest 40): what was told of each request once its
  notice was seen (Open, OK or Back), so a title is told of once, on this
  Roku.
- **Times** come as unix seconds, and the app writes them ("in 20 min",
  "3 days ago", "Nov 4", "9:05 PM") in the Roku's time zone; TMDB's days
  ("2026-11-04") are written as they are.
- **Discover's pictures** come through the server at the size they are
  drawn (`w`), with the scene's HTTP agent like library artwork.
- Launch beacons: `AppLaunchComplete` once the first screen is usable;
  `AppDialogInitiate` and `AppDialogComplete` around a sign-in at launch.

### Commands

In `apps/roku`, with Bun and Node 22 or newer. Bun runs the scripts, and
the tools (bsc, bslint) run on Node, since bslint's plugin loader needs
Node's `module.require`; so `apps/roku` has no `bunfig.toml`.

```powershell
bun install
bun run check     # bsc's compile check, bslint, the unit tests (CI runs this)
bun run smoke     # the whole app in brs-node, against a fake server
bun run bench     # the list work timed in brs-node
bun run preview   # the app in a browser, the keyboard as its remote
bun run images    # the nine-patch focus and watched-mark images
```

- **Preview** serves the app on http://127.0.0.1:8086/. Arrows, Enter
  (OK), Esc or Backspace (Back), Space (play/pause), `[` and `]` (rewind,
  fast forward) and `*` (options) are the remote. It uses the smoke run's
  fake server, which approves the code itself; `--server
  http://<address>:7539` uses a real one; `--port` changes the port.
- **Unit tests** run `components/lib` and `tests/*.test.brs` in brs-node;
  each `sub test_…()` is a test.
- **Smoke run** starts a fake server (`scripts/fixture-server.mjs`; its
  requests and TMDB titles, all made up, in `scripts/fixture-requests.mjs`)
  and walks every screen, saving each in `out/smoke/`. It fails on a
  BrightScript error, a request without the User-Agent, a token in an
  address, or a refresh token used twice. On the way it jumps down a
  library with its A to Z, saves a library's filters as a smart playlist,
  plays a collection of movies and a smart one of shows, and an artist's
  tracks, makes a playlist, plays a track from Now Playing's Up next,
  shuffles the queue and puts its order back, steps the music's sleep
  timer through its choices, plays a track from under an album's "Disc 2",
  asks for a movie and three seasons from Discover, approves a request,
  opens a title from its ready-to-watch notice, and checks that a server
  without requests is asked nothing but the summary.
- **brs-node limits:** its own fonts, no video or audio (Skip intro, Up
  Next and the sleep timers' ends have only unit tests), one fetch at a
  time, the odd dropped key, and no measure of a Roku's speed.
- **Images:** `bun run images` runs `scripts/make_images.py` (Python
  standard library). The home screen icons and splash screens come from
  `bun scripts/render-icons.mjs` at the repository root.

## Releases

The app is the `roku` unit in [RELEASING.md](RELEASING.md); its version is
the manifest's `major_version`, `minor_version` and `build_version`.

The manifest asks for SceneGraph 1.2, which every Roku OS from 9 runs.

## Known limits

- No playback speed or audio delay: the Video node has neither.
- Live updates need an `http://` server; with `https://`, Home and the
  requests' summary refresh once a minute, a title page with a request
  each 30 s.
- Audio other than the file's default isn't played as it is.
- Live TV has no channel numbers to type: Roku remotes have no number keys.
- Home has no hero over its rows, which the other TVs have.
- Not yet: an editable music queue.

