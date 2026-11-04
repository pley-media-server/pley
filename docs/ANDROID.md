# Pley for Android

One app, `com.formandelement.pley`, for phones, tablets, Android TV, Google
TV and Fire TV (Fire OS 7 and later), on Android 8 or later. It is native
Kotlin and Jetpack Compose, playing with Media3: `MainActivity` on phones
and tablets, `TvActivity` on TVs. Pley's other TV apps ([Kodi](KODI.md),
[Roku](ROKU.md), [Samsung](TV_PLATFORMS.md), [LG](LG.md)) follow its TV
screens. No store carries it yet, so you sideload it.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

Use the APK attached to an Android release (`pley-android-<version>.apk`),
or a debug build ([Commands](#commands)).

- **From a computer:** `adb install -r <file>.apk`. For a TV, run
  `adb connect <ip>:5555` first.
- **On a phone:** open the APK in a file manager, and let that app install
  apps when asked.
- **On a Fire TV:** turn on the developer options (select the device name
  7 times in Settings → My Fire TV → About), allow "Install unknown apps"
  for the Downloader app, and open the APK's address in Downloader. For
  adb, turn on ADB debugging there too.

Uninstall a debug build once before installing a release over it: they are
signed with different keys. Installing over the earlier web view app (0.1)
keeps its saved servers and asks you to sign in again.

## Using it

### Signing in

Phones sign in on the server's own page in the browser; TVs, and phones
without a browser, show a code and a QR code to approve at the server's
`/link` page, on a phone or computer. When the server requires two-step
sign-in, the sign-in screen says so, and whoever hasn't set it up yet does
it on that page as they sign in (for a code, on the device that approves
it).

Before its token goes to the server's address, the app has the server
prove it is the one signed in to, at that address
([API.md](API.md#telling-servers-apart)): for the access token, a proof
made within the half hour on the same networks (joining or leaving the
default network, or any Wi-Fi or Ethernet, asks for a new one, and so does
a request there that got no answer); for the refresh token and a profile's
PIN, one made then. What can't prove itself gets no token: the app shows
what it saved and says "Another device answers there."

### Profiles

A TV asks "Who's watching?" as it starts when the household has more than
one profile, unless Settings' Always start as names one; Switch profile
opens it at any time, on phones too. A profile with a PIN shows a lock and
asks it on a number pad: a wrong one clears the dots, too many say how
long to wait, and a PIN locked after 100 wrong ones in a row stops the
pad. Going on as the profile a TV was left on asks its PIN at a start. A
profile that can see more and has no PIN takes its password: a phone signs
in again in the browser, and a TV shows a code to approve on a phone,
where its owner picks it.

Always start as only skips the question: the TV starts as the profile it
was left on, but never past its PIN. When that profile has one, the start
opens "Who's watching?" on its PIN pad, and Back shows the other profiles.
Whether it has one is the profile's `has_pin` from `GET /api/auth/me`, saved
with the session, so the start reads it from the device; until the TV has
heard it for that profile, the start asks "Who's watching?".

### Phones and tablets

- **The player** shows its controls, the title with them, for 3 s as
  playback starts and as each later item begins (the next episode, a
  queue's next), where TVs show the title alone at the top; a tap shows
  them again.

### TVs and Fire TV

- Select opens, holding Select or Menu (☰) opens a title's actions in a
  panel over the page, which stays as it was, and Back goes back: to the
  page the rail opened as it was left, focus included.
- **The rail** lists, between the profile's avatar and Settings, Search,
  Home, Discover (or the requests), Live TV, the first six libraries in
  the server's order, **Libraries** when there are more (it lists them
  all), Playlists, Collections when there is no **Libraries**, Now Playing
  and the USB drive, each where it applies. Nothing makes way for
  another: entries are 40 dp tall on a 44 dp pitch, nine fit, and more
  scroll between the avatar and Settings, which stay put. Going down,
  focus moves until it is the fourth entry shown, then stays there while
  the entries move under it a whole entry a press, until the last ones
  show and focus goes on to the last; going up is the same backwards. An
  entry shows in the same place whichever way focus came, as in the app's
  lists, and none is ever cut in half. The closed rail shows the page's
  entry where focus on it would, so opening the rail moves nothing.
- **The player**: OK with the controls hidden shows them on the scrubber,
  as Up does (Down shows them on Play/Pause), and OK there plays and
  pauses. It seeks as YouTube's TV app does: Left and Right show the
  controls and move where it goes, 10 s a press (held, faster the longer
  it's held), and it goes there on OK, or by itself 5 s after the last
  key; Back lets it go. Fast forward and rewind (30 s) go without OK,
  400 ms after the last press. OK on the
  scrubber within two seconds of a seek plays on from there; later, it
  pauses. Play/Pause, the button or the remote's key, plays and pauses,
  and a seek waiting still goes. Up while a seek waits shows the chapters
  as a row of cards over the scrubber, on the one it was going to; the
  Chapters button shows them on the one playing. Left and Right go
  through them, OK goes to one, Down or Back closes them, and so does a
  media key's seek (Previous, Next, fast forward or rewind).
- A direction key held for 180 ms moves focus by itself, every 45 ms
  (about 22 a second), on the app's own clock: the system's repeats start
  later and come unevenly, so the app drops them. A held Left or Right
  slides a row on at that pace, a tile every 45 ms, under a ring that
  stays still at the row's left edge, the tiles keeping their resting
  look; when the key comes up, focus lands on the tile under the ring (or
  the next one the way the row was going) and the row settles there. Near
  a row's end, where tiles no longer reach that edge, focus steps across
  them instead. A held Up or Down glides a page of rows (Home, an item
  page, search) or a grid on a row at each step, each glide a little
  longer than the step, so the page never stops; in a grid the tiles keep
  their resting look under one ring that stays still where focus is
  going, until the key comes up. A press moves a row, a page or a grid
  over 150 ms, eased out, and so does Up from the rows of Home or an item
  page, or from a collection's grid, to its top. A tile's focus look
  comes in over 150 ms, eased out, and goes over 120 ms, eased in, with
  the native Samsung app's curves; where a held key's slide or glide
  stops, the still ring goes at
  once and the tile's look comes in that way. Elsewhere under a held key a
  tile's focus look changes evenly from one tile to the next, and
  backdrops and info blocks wait until focus has rested 400 ms. With
  Reduce motion a held key steps focus a tile or a row at a time and
  nothing slides.
  Search's recent searches move only when the next chip would pass the
  gutter or the safe area's edge, and then glide as a row does.
- A held key never leaves its list, as on a TV's home screen: it stops
  on a row's first or last tile, on a page's first or last row, and at a
  grid's edges, wherever the list is (Home, a show's page, search, a
  library, collections, playlists, Live TV, Settings). The rail, a
  page's hero, the A to Z jump and the chips or tabs above a grid take a
  press of their own, once the key has come up or at the edge. In a
  grid, Up and Down keep the column, held too. Coming back to a
  grid (Back from a title or its actions, Right from the rail, Down from
  the chips) lands on the tile focus left.
- On a show's page, Up from the episodes and Down from the buttons go to
  the tab of the season shown; another season shows once focus rests on
  its tab.
- Pictures wait while a key is held: the tiles a row slides past or a
  page glides past ask for none, unless the memory cache has them, and
  no backdrop is fetched ahead; where the key comes up, the tiles on
  screen and just past it load, with the backdrops around the focused
  tile. Every page's rows ask for the pictures ahead of them, a screen
  and a half, and stop asking for those the row has moved past.
- **Fire TV** runs the same APK and TV screens. The app spots it by the
  `amazon.hardware.fire_tv` feature and follows Amazon's player guidelines
  (asynchronous decoder queueing, surround from Fire OS's audio setting,
  Alexa's play and pause). Fire OS 16 asks for the nearby-devices
  permission before the app searches the network. An Appstore app's
  tile is the 16:9 icon in its listing
  (`deploy/stores/tv-banner-1280x720.png`, the mark and name); a
  sideloaded app's is drawn from the app's icon, squeezed into a square in
  the middle of a grey tile, whatever its shape or banner.
- **Watch Next** (Android TV and Google TV): the home screen's row shows
  the profile's Continue Watching, ten at most.
- **Dolby Vision and HDR** go by the display mode each video plays in: the
  one in use, or the one of its size the TV switches to for the frame rate
  (the device's match-content setting, or Settings → Playback; not for
  items under 10 minutes, nor once switching has timed out twice on that
  display). The app judges this before asking the server, so the server is
  told Dolby Vision only for a video the screen will show it in. A format
  turned off in the system's settings isn't used; Android says so only in
  a mode that lists it, so the app remembers it for each display. Dolby
  Vision the screen doesn't show plays as its HDR10, HLG or SDR base layer
  (a download of profile 5, which has none, on its Dolby Vision decoder),
  and the player's details say why ("HDR10 (Dolby Vision base layer, as
  this screen doesn't show Dolby Vision)"). Their Decoded line names what
  the decoder makes: HEVC and HDR10 for a base layer.

### Libraries

A library's Library tab sorts and filters its grid as the web app does:
watched or not, genre, year and, in movie and show libraries, resolution
(4K, 1080p, 720p, SD) and dynamic range (Any HDR, HDR, which takes in
Dolby Vision and HDR+, Dolby Vision, HDR+, HLG, SDR), the choices the
server lists where the library's titles differ in them. A title counts by
its best file, a show by its best episode.

- **Phones**: each filter is a chip over the grid that opens its sheet.
- **TVs**: Genre and Year are chips and rows of the Filter panel;
  Resolution and Dynamic range are rows of the Filter panel after Year,
  with a chip ("4K", "Dolby Vision") while one is chosen. Chips that don't
  all fit scroll with focus.
- Clear filters clears them all, Save as… keeps them in the smart playlist
  (`resolution=4k&dynamicRange=hdr`), and a library opens as it was left,
  filters included; a stored value the server wouldn't take is dropped.
- Posters carry their quality badges ("4K", "DV", "HDR") only while the
  grid is sorted by bitrate or filtered by resolution or dynamic range;
  sorted by bitrate, each also shows its bitrate and size. Recommended
  rows, collections, playlists, search and Home show none.

### Search

Search asks the server for up to 200 titles, 200 people and 200 Live TV
channels (`GET /api/search?limit=200`), the most it gives, and shows every
one: a row per group (Movies, Shows, Seasons, Episodes, Artists, Albums,
Tracks, Live TV, People) on phones and TVs, each scrolling sideways, with
no See all. TMDB's Discover row comes after them ([Requests](#requests)).

### Audio and subtitles

Unless you pick a track, the server picks the audio in the profile's
language and the best format this device plays. Tracks you pick in the
player are remembered for the show or movie; changing only the subtitles
leaves the audio unpinned.

| Setting | Where | What it does |
|---|---|---|
| Audio language, Subtitles, Subtitle language | Me (phones), Settings (TVs) | The profile's defaults |
| Sync | The subtitle menu | 0.1 s steps, up to 10 s either way, kept per file on the device |
| Subtitle style | Settings, or the player's subtitle menu while text subtitles show | Size, background, font, and position (bottom, or raised 5, 10 or 15 %) |
| Find subtitles… | End of the subtitle menu, while streaming | Searches online in the profile's subtitle language |
| Speed | Phones: player settings. TVs: a button at the player's left | 0.5× to 2×, pitch kept, this title only |
| Audio delay | Above the audio tracks (Left and Right on TVs) | Up to 5 s either way in 50 ms steps, this title only |

A chosen font overrides the system's caption style. The style is for text
subtitles only: picture subtitles (PGS, VobSub, DVB) keep their own look,
so the player doesn't offer it for them, nor with subtitles off.

Media3 changes the speed of decoded sound only, and a tunneled output
times the picture against the sound itself. So while the speed isn't
Normal, sound that goes to a receiver as it is (AC3, E-AC3, TrueHD, DTS)
is decoded on the device instead, and tunneled playback stops; an audio
delay stops tunneled playback too. Decoding the sound reloads the title
where it is, a short pause, and so does going back to Normal, where the
sound goes to the receiver as it is again; tunneled playback comes back
with Normal and no delay. A decoder counts only where it takes the sound
as it is (its channels and rate). When nothing on the device decodes that
sound, a stream asks the server again with the device's decoders only,
made ready beside the one playing: the server converts the sound and
copies the video. A download has no server to ask, and stays at Normal,
as does a stream the server can't convert (its transcoder away, or every
transcode taken); the speed menu says why, and the title plays on.

### Quality

The menu, Original and Automatic work as in
[QUALITY.md](QUALITY.md#the-menu). On Android:

- The choice is kept per device and network kind: Settings → Quality on
  Wi-Fi and Quality on mobile data on phones, Quality on TVs.
- Automatic asks the server's network memory while preparing; without an
  answer it uses its own, which a restart clears.
- Dropped frames at the original switch to the server's quality.
- With [several sizes](QUALITY.md#several-sizes-adaptive-bitrate), Media3
  moves between them, never above the screen's size.
- A new stream is prepared beside the playing one (`keep_previous`), so
  the last picture stays until it starts.
- Downloads play as they are.

### Live TV

This is how every Pley app shows Live TV; the [Kodi](KODI.md#live-tv),
[Roku](ROKU.md#live-tv), [Samsung](TV_PLATFORMS.md#live-tv) and
[LG](LG.md#live-tv) docs list only what differs. It appears only where
`GET /api/live` says `available` (a source with channels this profile may
watch), administrators included: in the web app's sidebar, the phone's
Libraries tab, after Home in the TV's rail. It comes and goes as sources
are added or removed (`live_channels_changed`). Sources, the lineup and
connections are set up in the web app's Settings → Live TV
([API.md](API.md#live-tv)).

- **Categories:** Favourites, Recently watched, All channels, then the
  regions in alphabetical order, each opening to its groups ("UK| Sport"),
  then More groups in the provider's order. The category, the view (guide
  or cards) and the last channel are kept per server and profile.
- **Search** looks across every channel by name, number or what's on, and
  offers the groups whose names match. The app's own search has a Live TV
  row too, which moves as the other rows do.
- **Paging:** channels come 100 at a time (`GET /api/live/browse`), and the
  guide asks only for the rows in view, six hours at a time, so a lineup of
  50,000 channels opens at once.
- **Live only:** no pause, rewind or seeking; play, pause, rewind and fast
  forward do nothing in the player.
- **Changing channel:** channel up and down stay in the category the player
  was opened from, wrapping round its ends. A typed number tunes that
  channel among all channels.
- **One connection per device:** the player tunes with this device's client
  id, so the server ends the channel watched before.
- **Every connection in use (`409`):** the player lists each connection's
  channel and who holds it, with **Watch here instead** where the server
  allows it (`can_take_over`).
- **A connection taken from you:** the server's `live_taken_over` event
  for the channel stops the player at once, and it shows **Live TV
  stopped** with the server's words (docs/API.md, "Taken over") and
  **Watch again**; a `410` from the stream says the same. It doesn't tune
  again by itself.

On Android the player reads the channel's MPEG-TS (`stream.ts`) as it
asks for the channel (`POST …/watch`), the server tuning for whichever
comes first, then asks for the session's `plan`: a channel the server sends
as it is plays that MPEG-TS from its first keyframe, as does one whose AC3
or E-AC3 sound the server converts, where the device decodes it or sends it
to its receiver as it is (Diagnostics lists both); anything else
plays the server's HLS. Taking over a connection, it reads `stream.ts` only once
the watch has answered, as only the watch takes the connection. Leaving
the player ends the session. A page of channels that didn't load is asked
for again once 10 s have passed, as the list moves. When the player fails,
the channel is tuned again twice, after 1 s and then 3 s, before the player
says why; not when the server refused it (every connection in use, the
channel stopped, gone or not allowed). Try again starts the count again.

On TVs, OK in the player opens the channel list, as Menu and Guide do;
holding OK shows the controls (the channel list and the favourite) over
what's on. Up and Down, Channel up and down, and Previous and Next change
channel. In the guide, Left and Right go programme by programme, back
through a programme that began before the guide a screen at a time, and a
half hour at a time on a channel without a guide; rewind and fast forward
(or Previous and Next) move it two hours, and the time now shows in a pill
over the now line. Holding OK, Menu or Info on a programme or a channel's
tile opens its details. Back goes up a step at a time: the guide back to now,
a search back to the category before, the guide or the tiles to the
categories, then the rail. A held Left stops at the start of the guide or of the
tiles, and a held Up at the top: a press of its own goes on to the
categories or the tabs, in the guide not within 0.5 s of Left moving.

### Requests

Where the server takes requests ([API.md](API.md#requests)), people ask
for movies and shows found on TMDB and administrators decide; Radarr and
Sonarr fetch what is approved. On phones and tablets:

- **Ways in**: Search before typing (Discover), Search's Discover row
  after the libraries' results, and Me's rows: Discover, and Your requests
  (Requests for administrators) with its number, which Me's tab carries
  too, in red while a request couldn't be sent. An account that may not
  ask and has no requests sees none of it.
- **Search's Discover row**, after the libraries' results, shows TMDB's
  titles that no library has, or why TMDB can't be searched (not
  answering, its key refused, no access), with Retry where trying again
  may help. See all opens every title TMDB finds for the words, with its
  badge, in a grid under "Results for “dune”", read page by page; Back
  goes back to Search. Words past the server's 200 characters are cut to
  them.
- **Discover**: a hero for the first trending title with its main action
  (when trending has nothing the profile may ask for, every row is read
  at once and the hero is the first title of the next one with any);
  rows (Trending this week, Popular movies, Popular shows, Coming to
  cinemas, Popular anime), each read as it comes into view and page by
  page; the person's requests. It has no search of its own.
- **A title's page** has the one main action (Request, Requested,
  Following, Tell me too, Open, Request more seasons, or why not), the
  requests for it with their stage, each season's state, and the cast.
  Request on a show opens "Which seasons?", the missing seasons ticked up
  to what the account may still ask for.
- **Requests**: views with their counts (Needs attention, oldest first,
  for administrators), movies or shows, and for administrators a person,
  their own and a title. Cards show the stage, the download's progress,
  why one was declined, and Watch once it's ready. Approve… picks the
  quality profile, folder and seasons; Decline takes an optional reason;
  Withdraw, Remove and Delete ask first. Pull to read the list again.
- **A library title** from a request shows its stage under its actions,
  and a show partly there offers Request more seasons.
- **News** (ready to watch, approved, declined; for administrators, new
  requests and ones that couldn't be sent) shows as a toast with Open
  while the app is open, never over the player; what came while it was
  closed is told at the next start.

On Android TV and Fire TV, as on the Samsung and LG apps:

- **Way in**: the rail's entry after Home, Discover where the profile may
  ask, else Your requests (Requests for administrators), with the number
  that waits there beside its icon, in red while a request couldn't be
  sent.
- **Discover**: the hero's main action and Details; TMDB's rows, each read
  as focus comes near, whose See all opens the whole list in a grid that
  reads five pages at most at a time and then offers Show more, as an age
  limit can leave page after page empty; the profile's requests, and the
  way to them at the top with their number. Back from the rows goes to
  the hero.
- **A title's page**: the one main action, Requested and Following
  focusable as every TV button is; Which seasons? is a side panel, opened
  from Request more seasons or a season's poster at that season.
- **Requests**: tabs with their numbers, the administrators' filters
  (movies or shows, Asked by, a title's words) and a card per request. OK
  on a card opens a side panel: Open where a library has it, the server's
  actions in its order, and Details. Withdraw, Remove and Delete ask first
  in the panel; Decline with a reason… takes the words on the TV's
  keyboard. Approve is plain: the server's settings choose the quality
  profile and folder.
- **Search's Discover row**, after the libraries' results: up to 20 of
  TMDB's titles that no library has, with See all; with nothing in the
  libraries, the line saying so above it. When TMDB can't be searched,
  the row says why, with Retry where trying again may help.
- **A library title** from a request shows its stage under its facts,
  with the download's progress, and a show with seasons left to ask for
  offers Request more seasons beside its play button.
- **News**: only what became ready to watch pops up, at the bottom right
  with the title's poster, Open and Close, once the remote has rested for
  1.5 s and nothing else holds focus (the open rail, a panel, a dialog,
  the keyboard, cards being sorted or moved); never over the player or on
  the requests, and coming back from the player counts as a press. It
  takes focus and gives it back, and goes by itself after 10 s untouched.
  The rest shows as the rail's number and in the lists.

On either, an app signed in before requests offers Sign in again on
Discover (a TV shows a new code to link), which asks for the `requests`
and `approvals` scopes the server now lists, and Settings → About credits
TMDB, as its terms ask. The summary follows the event stream; without
it, the app reads the summary every minute in the foreground and an open
list every 30 s. Download progress sends no events: a list, a title's
page, a library title or Discover's requests showing a download are read
again every 10 s while they show.

### Offline and downloads

The app needs no internet. On a phone whose Wi-Fi has no internet, traffic
to the home server still goes over the Wi-Fi (`LocalRoute`).

- **Server out of reach:** screens show what was saved ("Showing what Pley
  had saved. Reconnecting…") and recover on their own. A home address gets
  3 s to connect.
- **Playback that loses the server** plays what it has, then retries for
  90 s.
- **Downloads** (phones and tablets): Download is under Play, beside each
  episode and on a season; they are listed under Me → Downloads. A
  download is the original file (of several versions, the sharpest up to
  1080p the device plays as it is) with its text subtitles. A file the
  device can't play, or any file when Settings → Downloads → Quality picks
  a smaller size, is converted by the server first
  ([API.md](API.md#downloads)). Downloads wait for Wi-Fi or Ethernet
  unless Settings → Downloads allows mobile data. Progress made offline is
  sent when the server is back.

### Videos on a USB drive

On Android TV, Google TV and Fire TV, while a USB drive is plugged in, the
rail has **USB drive** after Now Playing, as in the native Samsung app;
no other entry makes way for it ([the rail](#tvs-and-fire-tv) scrolls).
The drive's page
lists the drive's folders, then its videos (MKV, MP4, M4V, MOV, TS, WebM,
AVI, MPEG), each sorted as a person sorts names ("Part 2" before "Part
10"); hidden files and what Windows and macOS leave on a drive aren't
shown. The page's title is the drive's make and model as it reports them
("Kingston DataTraveler 3.0") when the app can read them, else the
system's name for it ("SanDisk USB drive"), else "USB drive"; under it,
the folder's path ("/Films/2019"), at the drive's top "/" and how full the
drive is. A folder opens over the one before, with ".." first to go back
up. A video's line gives its kind, or where it resumes with a bar of how
far through it is, and its size. No drive, an empty folder and one that
can't be read say so centred on the page under an icon, as in the native
Samsung app; with nothing to focus (no ".." line) focus waits on that
message, showing nothing, so the rail closes over the page and Left opens
it. A video plays in Pley's player as it is, with no server, also while
the server is out of reach, the way a download plays: seeking, the
panels, the sleep timer and the subtitles' look are the same. The page is
in the rail, which shows once the TV is signed in to a server.

- **The drive** is the first mounted volume of `StorageManager`'s that
  isn't the device's own, removable ones first (some TV builds don't mark a
  USB drive removable), at the folder Android 11 and later give, else
  `/storage/<id>`; with none listed, a readable volume folder under
  `/storage`. The app follows the media broadcasts and, from Android 11,
  `StorageManager`'s callback, and looks again as the app or the page
  opens, for builds that send neither. Its make and model come from the
  disk `/proc/self/mounts` says it is mounted from, in `/sys`, as the
  native Samsung app reads them; most TVs keep both from apps, and the
  volume's own description stands in.
- **Leave to read it** is asked for as the page opens: `READ_MEDIA_VIDEO`
  from Android 13, `READ_EXTERNAL_STORAGE` before (Android 10 reads the
  drive as earlier versions do: `requestLegacyExternalStorage`). Refused,
  the page says so, with **Open settings**. A build for Google Play
  declares `READ_MEDIA_VIDEO` in the Play Console as the app's core use
  (playing the videos on the device); an APK installed by hand needs no
  declaration.
- **Tracks and chapters** are Media3's, as it reads the file: the audio it
  starts with is the one Media3 picks, the file's default where the device
  plays it. The file's own subtitles show as the player draws them.
- **Subtitle files beside it**, `.srt` and `.vtt` named after the video and
  then a language, "forced" and SDH in any order (`Film.en.forced.srt`;
  `Film 2.srt` is Film 2's), show through the app's own subtitles, and
  Sync moves them; it doesn't move the file's own tracks. Their text is
  UTF-16 or UTF-32 when it starts with that byte-order mark (Windows'
  "Unicode"), UTF-16 without one when its NUL bytes say so, else UTF-8,
  else Windows' Western code page (Windows-1252), as on LG TVs.
  From Android 11 the system decides which of a drive's files an app may
  read: a subtitle file it keeps from Pley isn't offered.
- **Where it stopped** is kept on the device, by the file's path; past 95 %
  it was watched, and it starts over next time.
- **Dolby Vision** profile 7 plays as its HDR10 base layer on the HEVC
  decoder, as there is no server to convert it; other profiles play as a
  server's stream of them does.
- **What doesn't play** says so: a file that only a conversion would play
  ("a file on a drive has no server to convert it"), a picture that doesn't
  start in 12 s, and a file that can't be read again or that stands still
  12 s once its picture has shown (the drive pulled out), as on LG TVs.

What only a server does isn't there: converting, the quality menu, seek
previews, Find subtitles…, saving track choices and the next episode.

## Building and testing

### Modules

Everything is in `apps/android`, sources in `src/main/kotlin`. Each module
sees only what it builds on.

| Module | What it holds | Builds on |
|---|---|---|
| `:app` | `PleyApplication`, `AppGraph`, the two launchers, Watch Next, the web view clean-up | everything |
| `:core:common` | Address rules, discovery, local network access, device profile, trace names | nothing |
| `:core:model` | The server's requests and answers, events, the capability report (plain Kotlin) | nothing |
| `:core:auth` | Sign-in: the browser page, the TV's code, tokens | common |
| `:core:network` | OkHttp clients, `PleyApi`, server events | common, model, auth |
| `:core:data` | Repositories, snapshots, servers, session, settings | network, auth, model |
| `:core:caps` | What the device plays: decoders, Dolby Vision, HDR, display modes, passthrough | common, model, Media3 |
| `:core:player` | The Media3 engine, playback sessions, subtitles, music | caps, data, network |
| `:core:downloads` | The download queue, its service, offline progress | data |
| `:core:designsystem` | Colours, icons, image loading, tiles (Compose foundation only) | common, model |
| `:core:screens` | One ViewModel per screen, shared by phone and TV; routes (`PleyKey`), `Navigator` | data, player, caps, auth |
| `:core:testing` | Fakes, JSON fixtures from [API.md](API.md), test rules | the contracts |
| `:ui:phone` | The phone and tablet interface (Material 3) | screens, designsystem, player |
| `:ui:tv` | The TV interface (tv-material, the rail, D-pad rules, the TV player) | screens, designsystem, player |
| `:testing:fixtures` | `FixtureServer`, a fake Pley server on 127.0.0.1 | model |
| `:benchmark` | Macrobenchmarks and the Baseline and Startup Profile generators | `:app` |

Rules the build keeps:

- `:ui:tv` never sees Material 3 or `:ui:phone`: their `MaterialTheme`s
  would mix.
- Screen modules meet only through `PleyKey` routes and the `Navigator`.
- Shared build setup is in `build-logic` (convention plugins) and
  `gradle/libs.versions.toml`.
- No native code, so the app runs on Fire TV's 32-bit userspace.
- The only exported components are the two launchers,
  `SignInRedirectActivity`, `MusicService` and `WatchNextImageProvider`
  (`CutoverTest` checks).

### Commands

In `apps/android`, with JDK 17 or newer (on Windows, Android Studio's):

```powershell
$env:JAVA_HOME = "C:\Program Files\Android\Android Studio\jbr"
.\gradlew.bat testDebugUnitTest lintDebug assembleDebug assembleStoreRelease   # as CI does
pwsh ..\..\scripts\check-android.ps1   # the release's libraries, native code and size
.\gradlew.bat :app:generateBaselineProfile   # by hand, on API 33 or later; commit the result
```

- Two flavors: `store` (Google Play and the Amazon Appstore, no
  self-updater) and `direct` (the APK to sideload, with an update check).
- Debug APKs: `app/build/outputs/apk/{store,direct}/debug/`.
- `-Ppley.composeReports=true` writes Compose compiler reports to each
  module's `build/compose/`.
- Baseline and Startup Profiles live in
  `app/src/main/generated/baselineProfiles`. A sideloaded install uses them
  only after the device's overnight compile; to apply them at once, run
  `adb shell cmd package compile -m speed-profile -f com.formandelement.pley`.

### Emulators

| Name | Image | For |
|---|---|---|
| `pley_test` | API 36 phone (`google_apis`) | The phone interface, UI tests |
| `pley-tv-34` | `system-images;android-34;google-tv;x86` on `tv_1080p` | D-pad and focus tests, profiles |
| an API 37 phone | Android 17 | The local network permission |
| (optional) | `system-images;android-36;google-tv;x86_64` | Fire OS 16's level |
| (optional) | `system-images;android-28;android-tv;x86`, 1 GB | Fire OS 7's level; can't play video |

```powershell
sdkmanager "system-images;android-34;google-tv;x86"
avdmanager create avd -n pley-tv-34 -k "system-images;android-34;google-tv;x86" -d tv_1080p
emulator -avd pley-tv-34 -gpu host
```

Emulators can't show HDR, Dolby Vision, display-mode switching,
passthrough or real decoder limits: check those on devices.

### A server to test against

Never test against a Pley server people use: signing in creates grants and
playing writes progress. Unit and UI tests use `FixtureServer`. For a real
server, start a disposable one:

```powershell
$data = Join-Path $env:TEMP "pley-android-test"
pley --data-dir $data reset-password admin <password> --admin
pley --data-dir $data --bind 127.0.0.1:7712 --no-discovery --no-tray
adb reverse tcp:7712 tcp:7712
```

The app then opens `http://127.0.0.1:7712` (an emulator also reaches the
computer at `http://10.0.2.2:7712`). Delete the folder afterwards.

### Performance

Measure on devices (a mid-range phone, a 1 GB Fire TV stick, a Chromecast
with Google TV HD), never on emulators. `TvStartupBenchmark` starts the TV
app cold ten times against the demo server built into benchmark builds, and
records the time to the first frame and to Home fully drawn, with a
Perfetto trace of each. `TvScrollBenchmark` and `PhoneScrollBenchmark`
record the frames (FrameTimingMetric: each frame's time on the main thread
and how late it was shown) while the app scrolls, five times each: on a TV,
Right held along Home's rows and Down held through a library's grid; on a
phone, Home's page and a row flung, and a library's grid. Each run starts
the app cold and goes to where it scrolls before measuring, and each
benchmark runs only on its kind of device. The demo server has no Live TV,
so the guide has no benchmark. Run them with adb on one device; a connected
Gradle task installs the app on every attached device.

```powershell
.\gradlew.bat :app:assembleStoreBenchmarkRelease :benchmark:assembleBenchmarkRelease
adb -s <device> install -r app\build\outputs\apk\store\benchmarkRelease\app-store-benchmarkRelease.apk
adb -s <device> install -r benchmark\build\outputs\apk\benchmarkRelease\benchmark-benchmarkRelease.apk
adb -s <device> shell am instrument -w -e class app.pley.android.benchmark.TvStartupBenchmark `
  app.pley.android.benchmark/androidx.test.runner.AndroidJUnitRunner
# The same with TvScrollBenchmark on a TV, PhoneScrollBenchmark on a phone.
```

On an emulator, add `-e androidx.benchmark.suppressErrors EMULATOR`. Traces
are in `/storage/emulated/0/Android/media/app.pley.android.benchmark` (on
Android 10 and earlier, `.../Android/data/app.pley.android.benchmark/cache`),
and the run's output names each.

On a device that already has Pley, signed with another key, build both with
`-Ppley.applicationIdSuffix=.bench`: the benchmarked app installs beside it
as `com.formandelement.pley.bench`, and the benchmarks (which clear the
app's data between runs) touch only that one.

Any build, release ones too, counts its frames while "Show frame rate" is
on: Settings → Diagnostics on a TV, Diagnostics on a phone, kept on the
device. For one run, the launch intent's `frames` extra turns it on (or
off), also when the app is already open:

```powershell
adb -s <device> shell am start -n com.formandelement.pley/app.pley.android.TvActivity --ez frames true
adb -s <device> logcat -s PleyFrames
```

Each half second something moves, the window shown (its frame metrics,
read on a thread of its own) gives a line, `fps=58 longest=21.4
missed=2 screen=Home`: frames drawn a second, the longest frame in
milliseconds from its vsync to its drawing done, and how many missed
their deadline (the deadline Android 12 and later give, else the
display's refresh). The screen is the route shown in the TV's or phone's
shell, else the activity (`TvPlayerActivity`). A box shows the same, on
a TV at the top right above the safe area, on a phone at the top centre
under the status bar: green with none late, amber with a few, red with a
frame in four or more. Dialogs and popups are windows of their own and
aren't counted. At rest nothing is drawn, so nothing is told. Off, none
of it runs.

A key held for a measure has to repeat as a remote's does: the TV app
takes a key that sends no repeats for half a second past the system's
first as up, so `adb shell input keyevent --duration`, which sends none,
moves focus for about a second, whatever the duration. The benchmarks'
journeys hold keys with the system's repeats (`Steps.hold`), injected
without UI Automator's accessibility connection, as a remote's come:
while an accessibility client is connected, Compose describes every
change of the screen to it.

A key held through a TV grid or along a row brings in a row of tiles
every 45 ms, and a frame has 16.7 ms. What keeps it smooth:

- Rows are made ahead of the screen between frames (each list's cache
  window). A held step that finds its row not made yet waits up to two
  frames for it, the glide under way spread over the wait so the grid
  goes on moving, rather than make it in the frame: once behind, steps
  that made their own rows left the frames no time to get ahead again.
  Each held glide stays a step and a quarter long, so focus never runs
  further ahead of the grid than the rows on the screen.
- A tile is few nodes, and costs little to make: its caption is one node
  drawing its two lines with the platform's paint, its picture draws in
  its own box, and its placeholder's title is laid out on a thread of its
  own. While a key holds pictures back, their requests are set aside off
  the main thread.
- A tile is drawn in its rounded shape, not clipped to it (a clip is a
  pass of its own for a TV's GPU, for each tile), on a layer of its own
  that scrolling moves rather than draws again, and its placeholder's
  gradient is one draw of one small picture shared by every title.

On an emulator, frame times (the benchmarks', the counter's or
`dumpsys gfxinfo`'s) say little about drawing: the emulator's GPU takes about the same time a frame
whatever is drawn, so its late frames are the GPU's, not the main
thread's. A change that draws less shows in HWUI's overdraw view
(`adb shell setprop debug.hwui.overdraw show`, then restart the app)
instead. What an emulator does show is compilation: the first flings after
a start run far slower, with many more janky frames, when the app isn't
compiled from its profile.

The Baseline and Startup Profiles come from `BaselineProfileGenerator`'s
journeys against the demo server: a cold start (the Startup Profile, which
puts what starting runs first in the dex), then, for the Baseline Profile
only, on a TV, with the
D-pad, signing in, Who's watching?, Home (a tile's actions, the rows), a
show's page (its episodes, OK held on one, its rows, the cast, a person),
search, a library's grid, Collections and a collection, Playlists and a
playlist, Settings (a choice's panel, the Libraries page), the rail and the
player; on a phone, by touch, Home, a show's page and its cast, the player,
a library's grid, search, a playlist and Me's Settings. Each journey runs
on its own kind of device and is skipped on the other, so generate with a
TV and a phone (or their emulators, API 33 or later) attached. Each step
waits for the screen it leads to, and a journey that goes astray fails
there, saying where. The phone signs in through the browser, or with a
code where it has none: on a new device with a browser, open it once
first, past its welcome pages.

### Recording an animation

A pull request that changes an animation shows it moving
([DEVELOPMENT.md](DEVELOPMENT.md), "Screenshots for a pull request").
`scripts/record-android.mjs` records the app as it is installed, any
build, with adb and FFmpeg: it sets the device's animator duration scale
to slow its animations as many times as it says (Compose follows the scale
at once), presses the keys given 300 ms apart, records the screen with
`screenrecord` for as long as it says, and puts the scale back. The frames,
cut to an area of the recording in its pixels, are saved in
`apps/android/build/rec-<time>-<model>`, each named with when it was shown
at the animation's own speed, for `scripts/mark-animation.mjs`.
The recording is the display's size, or its override size where
`adb shell wm size` shows one (a 4K TV's interface is usually 1920×1080);
the script says the size, and refuses an area that isn't inside it.
`screenrecord` gives a frame each time the screen changes, up to 60 a
second, but some devices give five or so a second while an animation plays
slowly; slowing it more gives more frames of it.

```sh
bun scripts/record-android.mjs 20 6000 0,0,960,1080 Right                         # 20× slower, 6 s, the left half
bun scripts/record-android.mjs 40 4000 0,0,1920,1080 Down OK --serial <device>    # adb's serial, where more than one is attached
```

## Releases

Versions, version codes and the release steps are in
[RELEASING.md](RELEASING.md) (the `android` unit). Store pictures and
listings are in [deploy/stores](../deploy/stores/README.md).

Release builds are minified, and signed with the key these environment
variables name (unsigned without them):

| Variable | |
|---|---|
| `PLEY_KEYSTORE` | The keystore file |
| `PLEY_KEYSTORE_PASSWORD` | Its password |
| `PLEY_KEY_ALIAS` | The key's alias |
| `PLEY_KEY_PASSWORD` | The key's password, when not the keystore's |

```powershell
$env:PLEY_KEYSTORE = "$HOME\keys\pley-release.keystore"   # and the others
.\gradlew.bat :app:assembleDirectRelease :app:bundleStoreRelease
```

This writes `app/build/outputs/apk/direct/release/app-direct-release.apk`
(to sideload) and `app/build/outputs/bundle/storeRelease/app-store-release.aab`
(for the stores). The Release workflow builds both with the key from the
repository's secrets: `ANDROID_KEYSTORE_BASE64` (the keystore,
base64-encoded), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and, when
it differs, `ANDROID_KEY_PASSWORD`.

Make the key once and keep it safe: every update must be signed with it.
With Play App Signing it is Google Play's upload key, which Play can reset
if it is lost.

```sh
keytool -genkeypair -v -keystore pley-release.keystore -alias pley \
  -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 pley-release.keystore   # the ANDROID_KEYSTORE_BASE64 secret
```

## Known limits

- Downloads are on phones and tablets only; TVs stream everything.
- No Watch Next row on phones or Fire TV.
- Live TV has no picture in picture, and the TV's display mode isn't
  matched for live channels.
- Videos on a USB drive are on TVs only. Their display mode isn't
  matched, as their frame rate is known only once they play; HDR10+ shows
  as HDR10 in the player's details; M2TS and MTS files aren't listed, as
  Media3 doesn't read their 192-byte packets.
