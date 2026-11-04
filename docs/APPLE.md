# Pley for Apple

Native apps for iPhone, iPad, Mac and Apple TV, in `apps/apple` (paths
below are relative to it). They are in development and not released yet.

- [What it is](#what-it-is)
- [Screens](#screens)
- [Finding servers](#finding-servers)
- [Plain HTTP and HTTPS](#plain-http-and-https)
- [Signing in](#signing-in)
- [Profiles](#profiles)
- [What each device plays](#what-each-device-plays)
- [Playing](#playing)
- [Downloads and offline](#downloads-and-offline)
- [Requests](#requests)
- [Top Shelf](#top-shelf)
- [Building and testing](#building-and-testing)
- [Code layout](#code-layout)
- [A server to test against](#a-server-to-test-against)
- [Known limits](#known-limits)

## What it is

Two apps built from one Xcode project, `Pley.xcodeproj`:

- **Pley**, one app for iPhone, iPad and Mac: native SwiftUI on each (not
  Mac Catalyst, not "Designed for iPad").
- **Pley for Apple TV**, a SwiftUI app built for the focus engine.

They share the bundle ID `com.formandelement.pley`, so they are one
purchase on every Apple device. The oldest systems they run on are iOS and
iPadOS 16, macOS 15 and tvOS 18. On the Mac the app runs in the App Sandbox
with outgoing network connections only.

Requests introduce themselves as
`pleyApple/<version> (<system> <version>; <device>)`, for example
`pleyApple/0.1.0 (iPadOS 16.7; iPad)` or `pleyApple/0.1.0 (tvOS 18.2; Apple TV)`.
The device part is the kind of device (iPhone, iPad, Mac, Apple TV), never
the name someone gave it.

The apps ask for local network access (to find and reach servers at home)
and browse for Bonjour's `_pley._tcp`. On iPhone, iPad and Apple TV they
play sound in the background (the `audio` background mode), for music and
Picture in Picture; it needs no entitlement.

## Screens

Both apps have the same screens, each built for its device. Signed in,
the app is in sections, each with a navigation of its own that Back goes
through; choosing the section showing goes back to its start, and the app
opens on the section shown last (a library's once the libraries are read,
with Home meanwhile). Switching profile, signing out or signing
in to another server starts every section again at Home.

- **iPhone**, and iPad in a narrow window (Slide Over, a third of Split
  View): a tab bar with Home, Libraries, Search, Playlists and Me. A library
  opened from the sidebar before the window narrowed shows under
  Libraries. While music has a queue, the mini player sits above the tab
  bar (on iOS 26.1 and later, in the tab bar's own accessory, whose words
  stop growing at the largest size below the accessibility sizes, as the
  tab bar's own do).
- **iPad** at regular width, and the **Mac**: a sidebar with Home, Live
  TV when it shows, Search and Playlists, then the libraries in the profile's order, each with its kind's symbol and a
  spinner while it is scanned, and at its foot the profile signed in, its
  picture, name and server, which opens Me. The Mac's window is at least
  800 by 560 points, and opens at 1100 by 720. While music has a queue,
  the mini player sits at the foot of the page beside the sidebar.
- **Apple TV**: seven tabs at most across the top, and no more than fit
  the screen's width, so none is cut off: Search (a magnifying glass
  alone, first, as in Apple's apps), Home, Live TV when it shows, a tab
  per library with its name while they all fit (the first that fit
  otherwise, and a Libraries tab for the rest; a long name fits fewer),
  Playlists, Now Playing while music has a queue (a music note alone),
  and Settings (a gear alone; it holds what Me holds elsewhere).
  VoiceOver reads the symbols' names. The focus engine and the Siri
  Remote move through it; Menu goes back, then to the tabs, then out of
  the app.

**On the Mac**, the menu bar has the app's commands, and Settings (⌘,)
and About Pley are windows of their own. The pointer on a tile grows it a
little (under Reduce Motion, outlines it instead) and shows a play button
over a video's picture. A right-click, or Control-click, on a tile, a
collection, a playlist or a library row opens its menu. With keyboard
navigation on (System Settings → Keyboard), Tab moves through the tiles,
Return opens the one with the focus and ⌘↩ plays it.

| Menu | Command | Keys |
|---|---|---|
| Pley | About Pley (a window of its own) | |
| Pley | Settings… | ⌘, |
| File | Add Server… (the servers, with the address field ready) | ⇧⌘N |
| File | Servers | |
| File | Switch Profile… (with the `profiles` scope and more than one profile) | ⇧⌘P |
| File | Sign Out… (asks first) | |
| View | Home | ⌘1 |
| View | Live TV, when it shows | ⌘L |
| View | Search (its field takes the keys) | ⌘F |
| View | Playlists | ⌘2 |
| View | The first seven libraries, in the profile's order | ⌘3 to ⌘9 |
| View | Discover, where requests are open (under Me) | ⇧⌘D |
| View | Your requests, or Requests for administrators (under Me) | ⇧⌘R |
| View | Reload (the page showing) | ⌘R |
| View | Larger Tiles, Smaller Tiles (Settings' grid size) | ⌘= (⌘+ too), ⌘− |
| View | Show or Hide Sidebar, and the toolbar's own | ⌃⌘S |
| Playback | Play "Title": the tile with the focus, or the title page showing, in a player window | ⌘↩ |
| Playback | Channel Up, Channel Down (the Live TV window that is key) | ⌘↑, ⌘↓ |
| Playback | Play or Pause, Back and Forward 10 Seconds, Skip Intro, Play Next, Quality, Audio, Subtitles, Find Subtitles…, Subtitle Sync, Subtitle Style, Speed, Sleep Timer, Chapters (the player window that is key) | |
| Playback | Play Music or Pause Music, Next Track, Previous Track (while music has a queue and no player window is key) | ⌘→, ⌘← |
| Playback | Now Playing (music's window) | |
| Window | Now Playing | ⌥⌘P |

An iPad with a keyboard has Search (⌘F) and Reload (⌘R) too.

**On Apple TV**, the remote:

- **Select** opens the tile, row or button with the focus; held (a long
  press), it opens a tile's actions, and on a library or a playlist's
  entry, Move.
- **Play/Pause** on a video's tile plays it, as Play would; anywhere
  else, it plays and pauses the music.
- **Menu** goes back a page, then to the tabs, then out of the app; in
  Move, it puts the library or entry back where it was.
- A row remembers the tile that had the focus: moving up or down to it
  again, or coming back to the page, puts the focus there.
- Home's facts about the tile with the focus fade in as the focus moves
  (at once under Reduce Motion), and its summary comes once the focus
  rests 0.3 s.

A short message (a toast) shows at the bottom of the window (on Apple TV,
top trailing, without taking the focus), and VoiceOver reads it out.
While the player is open, VoiceOver reads only the player, never the page
behind it.

- **Servers**: the servers found on this network ("On this network") and
  the ones opened before ("Your servers", which can be forgotten: that
  signs out of the server too), and "Add a server" by its address. A found
  server with the name of a saved one at another address says so. When the
  system refuses local network access, the screen explains how to allow it
  (on iPhone and iPad with a link to Settings).
- **Sign in**: the server's name and address, with a warning when it is
  reached over plain http outside the home network. iPhone, iPad and Mac
  offer "Sign in with your browser"; iPhone and iPad also "Sign in with a
  code". Apple TV shows a code straight away: a QR code of the server's
  `/link` page with the code in it, the code, the page's address and how
  long the code lasts. A code that runs out or is refused offers a new one.
- **Who's watching?**: after signing in, when the household has more than
  one profile, and from Me's Switch profile (where Cancel, or Menu on
  Apple TV, keeps the profile in use). Each profile shows its picture, or
  its initials on its colour; one with a PIN asks for it (4 to 8 digits,
  typed on the number pad), one that needs its password says how to choose
  it, and a locked one can't be chosen.
- **Home**: the server's rows for the profile, each a line of tiles in the
  row's shape: stills for Continue Watching and episodes, posters for
  films and shows, squares for music. A tile shows its picture, its title
  and a second line (the year, "S2 · E5", the artist), how far it was
  played, a check once watched, and a show's unwatched episodes; VoiceOver
  reads it as one ("Arrival, 2016", "watched"). Tiles grow with the text
  size, up to 1.6 times. Pictures are fetched at the width they show,
  rounded up to a multiple of 64 pixels, and fade in except under Reduce
  Motion. At most 6 are fetched at once, the newest asked for first, and
  one scrolled away before it comes isn't fetched. They are kept in memory
  up to 32 MB on a device with 3 GB of memory or less (otherwise 64 MB on
  Apple TV and 96 MB elsewhere), half of that when the app goes to the
  background, and none after a memory warning. While loading, still grey
  rows show; a load that fails says why with Try again. With no rows,
  Home says why: no library this profile may open, the libraries being
  scanned, or nothing in them yet. Rows a library can list (Recently
  Added, Recently Released, Top Rated) have **See all**, which opens the
  library listed the same way. Leaving Continue Watching takes the tile
  out at once. After it loads, Home reads the first three Continue
  Watching titles' details, which starts the server reading their video
  ahead. iPhone and iPad pull to refresh, the Mac has Reload (⌘R).
  Choosing a tile opens its page. On Apple TV, above the rows, what the
  tile with the focus is: its title (a show's, for an episode), the
  episode, the year, rating, running time and quality, the time left, and
  once the focus rests 0.3 s, its summary in three lines.
- **A title's actions**: a long press on a tile (iPhone, iPad and Apple TV,
  with Select), or a right-click or Control-click on the Mac, opens its
  menu; VoiceOver offers the same list as actions. By kind: Play, or
  Resume and Play from start when part played (a show or season plays its
  on-deck episode); Mark as watched or unwatched; Remove from Continue
  Watching, from that row; Add to or Remove from My List, for movies and
  shows; Add to playlist; Go to show and Go to season, for episodes and seasons; Open, for
  music. Play/Pause on the Siri Remote plays a tile. A mark or My List
  shows at once and goes back, with a toast saying why, when the server
  refuses. Nothing for administrators is offered.
- **Libraries** (the iPhone's tab, Live TV first; the sidebar on iPad
  and Mac; a tab on Apple TV when the libraries don't all fit): each library with what it holds ("412
  movies", "37 shows", "128 artists") and whether the server is scanning
  it. With two or more, they can be put in the profile's own order: Edit
  and a drag on iPhone and iPad (the sidebar's Edit, or a drag, on iPad
  and Mac), Move from a long press on Apple TV (up and down move it,
  Select puts it down), and Move up and Move down for VoiceOver. Each
  move shows at once and is saved in order; Home, the sidebar, the tabs
  and the profile's other devices follow. "Use the Server's Order" goes
  back to the server's.
- **A library**: **Recommended** (the library's own rows, then its
  collections), **Library** (every title, in a grid) and **Collections**
  (not for music). The grid reads 100 titles at a time, the page
  showing and the next once it is 70 % through, keeps at most 2,000
  titles, and lets go of the pages furthest away. **Sort** offers the
  library's sorts (title, year, date added, rating, your rating, release
  date, duration for movies, last played, bitrate), each with its own
  order and the other one beside it; sorted by bitrate, each title's
  second line is its file ("1080p · 6.6 Mbps · 50.4 GB"). **Filter**
  (not for music): unwatched or watched, a genre, a year, and Clear; on
  iPhone a sheet, on iPad and Mac a menu, on Apple TV a menu in the row
  of buttons. A line under the tabs says how the titles are listed ("Year,
  descending · Unwatched"). Sorted by title, with more than 30 titles and
  at most 2,000, the letters "#" and A to Z jump through the grid: down
  the side on iPhone, iPad and Mac (a tap or a drag; VoiceOver's "Jump to
  letter", swiped up and down), a row above the grid on Apple TV. A letter
  without titles is dim and goes to the next that has some. The page
  opens as it was last left, for each server, profile and library, unless
  See all asks for a view. Filters that hide everything say so, with
  Clear filters. **Save as Smart Playlist** (in More on iPhone, iPad and
  Mac, a button in the row on Apple TV) keeps the sort and filters
  showing as a smart playlist.
- **Me** (Settings on Apple TV): the profile's picture and name ("Managed
  profile" for one without a password of its own), Switch profile (when
  the household has another profile), Discover and Your requests
  (Requests for administrators, with its number) where requests are open
  ([Requests](#requests); not on Apple TV), the profile's **audio and subtitle
  defaults**, Downloads (iPhone, iPad and Mac: "Watch without your
  server", [Downloads and offline](#downloads-and-offline)), History,
  Settings (on iPhone and iPad; the Mac's are in its Settings window, ⌘,),
  the server's name and address, the servers, and Sign out after asking. The defaults are the server's for the profile
  (`PUT /api/me/track-defaults`), so every device follows them: the audio
  language ("The file's default", or one of 34 languages), Subtitles (Off,
  "Only for foreign dialogue", "When the audio is in another language",
  Always) and, unless Off, the subtitle language ("Same as the audio
  language", or one of the languages). A change shows at once and is
  saved, with a toast; one the server refuses goes back, saying why. A
  sign-in that may only play (a code given for a TV) shows them without
  letting them change. A profile's name, picture and PIN are changed in
  Pley on the web.
- **Search** (the iPhone's tab, the sidebar's row on iPad and Mac, the
  first tab on Apple TV): the system's search field (in the Mac's
  toolbar; on Apple TV, the system's keyboard). What is typed is searched
  0.25 s after the last key (`GET /api/search`: the 40 best titles and up to 12 people), and only
  the latest search's answer shows. The results come in rows by kind, each
  with its count, titles first, as in every app: Movies, Shows, Seasons,
  Episodes, Artists, Albums and Tracks, in the shape of Home's tiles,
  then Live TV's channels (up to 12, by name, number or what's on), which
  open the live player in All channels, then People (round, with their
  initials when they have no photo). Each row keeps the server's order,
  closest match first.
  Choosing a result opens its page. With nothing typed, the profile's
  **recent searches** show, newest first: the last 10 searched or opened,
  kept on the device for each server and profile, each searched again
  when chosen, and Clear. Nothing found says so ("No results for
  “clip”"); a failed search says why, with Try again. On iPhone, iPad
  and Mac, where requests are open, Discover heads the page before
  anything is typed, and TMDB's titles that no library has follow the
  results in the Discover row ([Requests](#requests)).
- **History**: the profile's plays, newest first, 15 at a time with Show
  more (`GET /api/history`). Each says what played ("The Office", "S2 · E3
  · Office Olympics"), when ("3 hours ago", "yesterday") and on which
  device, with its picture; choosing one opens its page. Swiping (a long
  press on Apple TV, a right-click on the Mac) removes a play
  (`DELETE /api/history/{id}`): it goes at once, and comes back with a
  toast when the server refuses.
- **Settings**: this device's settings, kept on it (`UserDefaults`), the
  same for every server and profile. On iPhone and iPad a page from Me;
  on the Mac the Settings window with Playback, Subtitles, Browsing and
  Downloads tabs; on Apple TV under the profile's in the Settings tab, each row
  opening its choices.
  - **Playback**: Quality ("Quality on Wi-Fi" and "Quality on mobile
    data" on a device with mobile data): Automatic, Original or a quality,
    the one each playback starts with ([Quality](#quality)); and Play the
    next episode automatically (on by default), which off leaves Up
    Next's card without its countdown.
  - **Subtitles**: Size (System, 75 % to 200 %), Background (System,
    None, Dark box), Font (System, Sans serif, Serif, Monospaced, and
    Casual where the device has it) and Position (Bottom, or raised 5, 10
    or 15 % of the video's height), with a preview; [Subtitles](#subtitles)
    says how they meet the system's caption style.
  - **Browsing**: Grid size (Small, Medium, Large: the tiles in a
    library's grid, 0.8, 1 and 1.25 times their size), Use the server's
    library order, and on iPhone, iPad and Mac Sign in privately (on by
    default: the browser sign-in keeps no cookies, so it asks for the
    password each time; off, it can use the browser's sign-in to Pley).
  - **Downloads** (iPhone, iPad and Mac): Quality (Original, "1080p · 8
    Mbps", "720p · 4 Mbps", "480p · 1.5 Mbps"; Original downloads the
    file unchanged when this device plays it, and the lower ones, smaller,
    are made by the server first), Download on mobile data (iPhone and
    iPad; off by default, so downloads wait for Wi-Fi), what the downloads
    take and what is free, and Delete all downloads, after asking (every
    server's and profile's on this device).
  - **Always start as** (Apple TV, when the household has more than one
    profile): "Ask who's watching" (the default) or a profile, kept for
    each server.
  - **Diagnostics**: the device's class, what it decodes and shows, and
    the caps report sent with each playback request, as JSON, with Copy
    (not on Apple TV).
  - **About**: the version and build ("Pley 1.0 (build 12) for iPhone"),
    that the app holds no third-party code, and Pley's licence; on
    iPhone, iPad and Mac, TMDB's notice ("This product uses the TMDB API
    but is not endorsed or certified by TMDB."), as its terms ask.
- **A title's page** (movies, shows, seasons and episodes): the backdrop
  (at most 1536 pixels wide on a device with 3 GB of memory or less,
  1920 otherwise), the poster, and the title with where it sits: an
  episode's show and season, a season's show, as links. Under the title,
  one line of facts read by VoiceOver as one: the year (a show's years,
  "3 seasons"), the running time, the content rating, the critics' rating
  and the file's quality badges ("4K", "DV", "Atmos"). Then **Play** or
  **Resume**, with how far it was played and the time left (a show or
  season plays its next episode: "Play S1 · E3"), **From start** when
  part played, **Watched**, **My List** (movies and shows), **Trailer**
  when there is one, and **More** with the rest of the title's actions.
  The profile's own rating, in half stars: a tap on iPhone, iPad and Mac
  (the left half of a star for a half star; a scroll that starts on the
  stars only scrolls), the arrow keys and Delete on the Mac, one
  adjustable element for VoiceOver ("Your rating, 3.5 stars"), a list on
  Apple TV. A movie's or episode's pickers: the
  version when there are several, the audio track, with **Automatic**
  first ("Automatic (English)": the profile's languages pick the track,
  which a track picked here replaces), and the subtitles, with Off; each
  choice is saved on the server, for the whole show when it is an
  episode's. The tagline, summary and genres, and the collections it is
  part of, each opening its page. A show's seasons and the episodes of one (the season of its
  next episode, else the first that isn't Specials), each with its still,
  progress or check, number and title, length and air date, and summary;
  choosing one opens its page, its picture plays it, and a long press
  gives its actions. A season or an episode lists its season's episodes.
  Then rows of extras (which play), the cast (Guest Stars on an episode),
  each opening the person's page, and More Like This; crew (Created by,
  Director, Writers, four names then "and 2 more", each opening their
  page); details (original title, release date, studio or network,
  plays, versions); and **Media info**, each file's facts and streams,
  behind a disclosure (a button on Apple TV). Watched, My List and the
  rating show at once and go back, with a toast saying why, when the
  server refuses. When Play gets the pointer (iPad, Mac) or the focus
  (Apple TV), the server reads ahead what it plays, at most once in two
  minutes. After the player closes, and when the server says the title,
  its show or an episode listed changed, the page reads it again. A title
  that is gone says so. An extra opens its movie's or show's page, a
  track its album's.
  - **iPhone**: one column. At the accessibility text sizes the poster
    sits above the title, and the buttons and an episode's picture and
    words stack.
  - **iPad and Mac**: from 700 points wide, the poster beside the
    title's column (but one column at the accessibility text sizes), and
    the rows across below.
  - **Apple TV**: the backdrop fills the screen behind the title, its
    facts, summary and a row of buttons (Play, From start, Watched, My
    List, Trailer, the rating, the version, Audio and Subtitles, More);
    below come the seasons as a row of buttons (and Open Season), the
    episodes as a row of cards, then extras, cast, More Like This, and
    details with Media info. Play/Pause on an episode's card plays it.
- **A person's page**: their photo (or initials), name and "3 movies · 1
  show", then a row for each thing they did (Acting, Directing, Creating,
  Writing, Producing), each title with its year and their part ("as
  Louise Banks", "Director"). People come from a title's cast and crew,
  which a metadata source (TMDB) gives.
- **Music**: an artist's page, with their picture, "4 albums · 40 tracks",
  what the server says of them and their albums, newest first; and an
  album's page, with its cover, its artist (a link), the year, "10 tracks ·
  42m", the profile's rating and its tracks (number, title, the track's
  own artist when it isn't the album's, length), with disc headings on an
  album of several discs. An album opened from a track marks that track
  and scrolls to it (on Apple TV, gives it the focus). Both have **Play**
  and **Shuffle**: an album plays its tracks in order, an artist every
  album's tracks, newest album first (5,000 tracks at most). Choosing a
  track plays the album from it; the track playing shows a speaker in
  place of its number (VoiceOver says "playing" or "paused"), and
  choosing it pauses or plays on. [Music](#music) says how it plays.
- **Collections**: a library's collections in a grid, each tile with its
  poster, how many titles it holds, and a TMDB badge (made from TMDB's
  movie collections) or a Smart one (filled by a filter). A collection's
  page: its backdrop and poster, title, badges, "12 movies" with how it is
  sorted or, for a smart one, its filter in words ("Comedy · Unwatched ·
  Year, newest first"), its summary, **Play** and **Shuffle**, then its
  titles. Play plays a collection of movies in its order as a queue;
  Shuffle starts the queue at a random movie. A collection of shows plays
  the first show's next episode (Shuffle, a random show's).
- **Playlists** (the iPhone's tab, the sidebar's row on iPad and Mac, a
  tab on Apple TV): the profile's playlists by title, each as a mosaic of
  its first four pictures with "12 items · 3h 12m", and **New Playlist**.
  A playlist's page: its mosaic, title, the count and length, a smart
  one's filter in words, **Play** and **Shuffle**, then its entries, each
  with its number, picture, title, what it is from ("The Office — S2 ·
  E3") and length. Choosing an entry plays the playlist from there; a
  long press (a right-click on the Mac) also has Go to Page, Move Up,
  Move Down and Remove. Entries move with Edit and a drag on iPhone and
  iPad, a drag on the Mac, and Move on Apple TV (up and down move the
  entry, Select puts it down, Menu puts it back); VoiceOver has Move up
  and Move down. A move shows at once; while one is being saved, only the
  latest order waits to go next, and a refused one puts the order back
  with a toast. A removed entry goes at once and the toast offers Undo
  for 5 seconds (on Apple TV, Undo is a button beside Play, since the
  toast takes no focus); the server hears of it after that. Rename and
  Delete (after asking) are in More (buttons on Apple TV). A smart
  playlist can't be changed: it plays what its filter finds. A music
  playlist plays in Now Playing ([Music](#music)) as a video one plays in
  the player, and Go to Album is in an entry's menu.
- **Add to playlist** (a title's actions): a sheet (a panel down the
  right on Apple TV) that names what is added in full ("The Office — S2 ·
  E3 — Office Olympics", "Heat (1995)", "2 items"), a title for a new
  playlist made with it, and the profile's playlists of its kind (video
  or music; smart ones left out). A show, season, album or artist adds
  all its episodes or tracks. The toast says how many the server added.
  **New Playlist** asks for a title (up to 200 characters) and, empty,
  whether it plays Movies & TV or Music; the playlist made opens.
- **The player**: the system's player on each device, below.
- **Live TV** (when an administrator added an IPTV provider and opened
  it to the profile, `GET /api/live`; read at sign-in, at a profile change,
  when the lineup changes and when the event stream connects again): the
  first row of the iPhone's Libraries (with its channel count), the
  sidebar's row after Home on iPad and Mac (View → Live TV, ⌘L), and a tab
  after Home on Apple TV. The categories (Favourites, Recently watched, All
  channels, each region's groups under it when two regions or more have
  groups, then More groups) are a sheet on iPhone and a column on iPad, Mac
  and Apple TV; a search lists the groups it names first, then the
  channels whose name, number or programme on now has it. Channels come a
  page of 100 at a time as the list scrolls, ten pages kept at most, so a
  lineup of tens of thousands costs what is on screen. Two views, kept per
  server and profile with the category: the **guide** (channels by time,
  48 hours from the half hour before now, fetched six hours at a time for
  the channels loaded; on iPhone, iPad and Mac it scrolls across with the
  channel column pinned and a red line at the time now, moved every 15
  seconds, at once under Reduce Motion; on Apple TV it shows two hours,
  and moving the focus past an edge moves it half an hour), and
  **channels** (cards with the logo, number and name, what's on with its
  progress and what's next, and the star). Logos are asked for at the size
  they show. The list opens again on the channel watched last. A
  programme opens its picture, time, Live and New, what it is about, and
  Watch now while it is on. A star shows at once and goes back, with a
  toast saying why, when the server refuses it. A profile without Live
  TV, or a server without channels, says so, as the web app does.
- **The live player**: full screen on iPhone and iPad, a window of its own
  on the Mac. It asks the server for the channel with this install's
  client id (which ends what it watched before, so changing channel never
  holds two of the provider's connections) and plays the keyed HLS
  address the answer gives. Live plays as it airs: nothing pauses or seeks.
  A tap shows, for 3 seconds, Close, the channel's logo, number and name,
  what's on with its progress and what's next, the star, channel up and
  down (through the category it was opened in, round its ends; Recently
  watched in the order it had when the player opened), Channels (the
  category's list, and Other channels for another), AirPlay and "Live". A
  keyboard's digits tune a number (up to five, 1.4 seconds after the last,
  or Return), and ↑ and ↓ change channel; on the Mac, F fills the screen,
  ⌘W leaves, and the Playback menu has Channel Up (⌘↑) and Channel Down
  (⌘↓). On Apple TV a banner with the channel and what's on shows for 5
  seconds after each change; up and down change channel, Select opens
  Channels, Channel number, Favourite and Back to the guide, Play/Pause
  shows the banner, and Menu hides the overlay, then leaves. The number
  pad (1 to 9, Delete, 0 and OK, each named for VoiceOver) tunes on OK,
  since moving to the next key can take longer than a keyboard's wait;
  "No channel 123" shows for 2 seconds when none has it. When every
  connection of the channel's source is in use, the player says so, lists
  what they watch, and offers **Watch here instead** on one it may take. A stall shows a spinner after 0.7
  seconds; a channel with no picture shows its logo and "Sound only".
  When AVPlayer fails, the session's plan is asked once: a stop at the
  server says why, anything else tunes again. Leaving ends the session at
  the server at once. In the background the channel and its connection
  are let go, and back in front it tunes again from the live edge. One
  playback at a time: a video closes the live player, and Live TV closes
  a video.
- **The mini player and Now Playing** (while music has a queue): the mini
  player at the foot of every page on iPhone, iPad and Mac, which opens
  Now Playing (a sheet on iPhone and iPad, a window on the Mac, ⌥⌘P), and
  the Now Playing tab on Apple TV ([Music](#music)).
- **Downloads** (iPhone, iPad and Mac, Me → Downloads): the profile's
  downloads on this device, with what they take and what is free
  ([Downloads and offline](#downloads-and-offline)).
- **Discover**, **Results for “dune”** (Search's See all), **a TMDB
  title** and **Requests** (iPhone, iPad and Mac, from Me and Search):
  finding movies and shows, asking for them, and following and deciding
  requests ([Requests](#requests)).

### Accessibility

- **Dynamic Type** (iPhone, iPad and Mac): text uses the system's text
  styles, and tiles grow with it up to 1.6 times. At the accessibility
  sizes, the iPhone's grids show two columns; a row's title goes above its
  See all, a show's Seasons above its season's link, and Play above
  Shuffle; a title page's header, buttons and facts stack; captions take
  three lines; episode rows wrap; and the iPad's sidebar is wider. Live
  TV's guide gets wider with the text, so a programme's title still fits;
  the mini player puts its words and buttons on lines of their own, Now
  Playing's buttons and Up next's rows wrap, and a download's row grows
  with its text. Nothing has a fixed height that clips its text.
- **VoiceOver**: each tile is one element ("The Office, season 2, episode
  5, Glasshouse", "26 minutes left", "Opens its page") with its menu as
  actions; rows' titles are headings; the A to Z strip ("Jump to letter")
  and the star rating are adjustable; toasts are read out; pictures are
  hidden; and while the player is open, only the player is read. A guide
  programme reads its channel, number, title, time and whether it is on
  now ("BBC One, 101, News at Six, 6:00 to 6:30 PM, on now"); a channel's
  row is one element with its star beside it; the live player's buttons
  and Apple TV's number pad keys ("One", "Delete", "Tune") are named, and
  what went wrong with a channel is read out as it shows. The mini player
  is one element ("Now playing, Sunson, Nils Frahm") with Play or Pause,
  Next and Open as actions, and Now Playing's scrubber is adjustable. A
  download's row reads its title, state and percentage, and the offline
  banner is read out when it shows. Apple TV uses the same labels.
- **Reduce Motion**: no tile growing under the pointer (an outline
  instead), no pictures fading in, no fade between Home's facts on Apple
  TV, and no animated changes; loading rows don't pulse. The guide's line
  at the time now jumps instead of sliding, and the live player's
  controls and banner show and hide without moving.

### Updates from the server

While the app is in front and signed in, it reads the server's events
(`GET /api/events`, server-sent events), through the same client as every
request, so the token and the plain-http rule are the same. The events
that come within 0.3 s of each other are gathered, so a play reaching
90 %, which sends two, reads each screen once. A watched mark, My List, a
library's items or the libraries changing reads Home, the libraries and a
library page showing those titles again; scans show on their library as
they start and end. The app's own marks and orders go through the same
path, so other screens follow them even before the server's event comes.

A line longer than 64 KiB drops the connection. The stream connects again
when it ends or fails, after 1, 2 and 5 seconds and then every 15; `429`
(too many streams for the sign-in) waits 30 seconds. A connection made
again, or the app coming back to the front, reads everything showing
again, since events were missed. It stops when the app goes to the
background, and for good when the sign-in ends or the server refuses the
stream.

When the server stops this device's playback (`session_stopped`, from
another device or by the administrator), the player closes and a toast
says so ("Playback was stopped by the server's administrator: Bedtime").

At start, the app opens the server used last when it is still signed in
to it, and shows the servers otherwise. On iPhone, iPad and Mac it opens
on the profile used last. On Apple TV, a shared screen, it asks "Who's
watching?" at each start when the household (as last seen on this
device) has more than one profile, unless Always start as names one: the
app then opens as that profile, on the section shown last, switching to
it when it is another and needs no PIN, and asks when it needs one.
At that start, going on as the profile in use asks for its PIN when it
has one, so the profile used last is no way past it; Switch profile's
Menu, and the choice right after signing in, keep the profile in use
without it. A server opened from the servers, after a start that
couldn't reach it or from another server, goes through the same gate.
When a sign-in ends without the
person asking (the server ended it, or the address rule below), the app
goes back to that server's sign-in and says why.

## Finding servers

The apps find servers on the home network by Bonjour, which is off on a
server until its administrator turns it on in Settings → Network. Pley
servers then announce `_pley._tcp` ([API.md, Discovery](API.md#discovery)),
and the apps browse for it with Network.framework's `NWBrowser`. They
don't send the `pley?` broadcast the other apps use, so they need no
multicast entitlement. A server that isn't found (Bonjour off on it,
another network, a router that drops it) is added by its address.

The first browse makes the system ask for local network access, with the
reason from the app's Info.plist. If it is refused, the search reports it
as refused rather than as an empty network; Settings → Privacy & Security
→ Local Network turns it back on. Adding a server by its address still
works for servers beyond the home network.

Anyone on the network can announce a `_pley._tcp` service, so an
announcement is only a lead:

- **The TXT record is bounded**: `pley` must be `1` and `id` 1 to 64
  letters, digits and hyphens; the name is cut to 64 characters without
  control or format characters, and falls back to the instance name; a
  version is at most 32 printable characters. A record with more than 16
  keys or 1,300 bytes is ignored, and so are servers beyond the first 32.
- **The address must be private**: the service is resolved by opening a
  connection to it, an IPv4 address preferred, and IPv6 used only when it
  isn't link-local. An address outside the private ranges below is
  dropped.
- **The server must confirm it**: the app asks `GET /api/server` at that
  address, at most 4 at a time, and lists the server only if it answers
  as Pley within 5 seconds. The name and version shown are from that
  answer; the id is from the TXT record, and two announcements with one id
  show once. An announcement that isn't confirmed is checked again after
  5, 15 and 60 seconds while it is still announced, since one slow answer
  would otherwise hide the server until it announces something new.

A server that says goodbye (it stopped, or its announcement went) leaves
the list. Servers are listed by name.

## Plain HTTP and HTTPS

A server is reached over https, or over plain http on the home network.
Plain http carries sign-in codes and tokens only to private addresses:
this device (127.0.0.0/8, `::1`), the home network's ranges (10.0.0.0/8,
172.16.0.0/12, 192.168.0.0/16), link-local addresses (169.254.0.0/16,
`fe80::/10`), Tailscale's 100.64.0.0/10 and IPv6 unique local addresses
(`fc00::/7`), the same rule as the Android app's.

URLSession sends a request as soon as it connects, so the rule is checked
on both sides of it:

- **Before**: an IP address is checked as it is; a name is looked up, and
  every address it has must be private. Otherwise nothing is sent.
- **After**: the address the connection actually reached (from the
  request's metrics) must be private too. If it isn't, the answer is thrown
  away, and a sign-in whose token went with the request ends on the device.

https needs nothing more: certificates are checked by the system, against
its own certificate authorities only.

Tokens go in the `Authorization` header only, never in an address, and
only to the origin they were given for (the same scheme, host and port).
The API follows redirects within that origin only: never from http to
https or back, and never to another host or port. The sign-in endpoints
follow none. No cookies are kept or sent, and nothing is cached on disk.
Every answer has a size limit: 64 KiB for the server's name and the
sign-in endpoints, 8 MiB for the API's JSON and 10 MiB for pictures, which
are shown only as JPEG, PNG, WebP or GIF.

The player's media follows the same rule. AVPlayer is given only a keyed
address the server named in its answer (`/api/media/{key}/…`, on the
server's own origin), checked before AVPlayer has it; over plain http,
its host must pass the check above. The key in the address stands for the
session, so AVPlayer sends no token and no header is added to its
requests.

Apple's App Transport Security allows plain http to IP addresses, names
without dots and `.local` names (`NSAllowsLocalNetworking`); the apps ask
for nothing wider. A server whose http address is a name with dots
(`nas.home.arpa`) is opened by its IP address or over https instead.

## Signing in

The apps sign in with OAuth as Pley's own apps
([API.md, Apps](API.md#apps-oauth-21)), each with its client ID built into
every server: `https://pley-media-server.github.io/pley/oauth/ios.json`,
`tvos.json` and `macos.json`. They ask for `library play playlists
profiles` (the ones the server offers), and `requests` and `approvals`
only where the server's metadata lists them (an older server refuses a
whole sign-in for a scope it doesn't know), and hold a viewer's tokens
only, whoever signs in.

Before signing in, the app reads the server's metadata
(`/.well-known/oauth-authorization-server`) and keeps it only when its
issuer is exactly the address the app used and every endpoint is on that
address. A server without it is too old for the app.

- **iPhone, iPad and Mac** sign in on the server's own page, in the system's
  sign-in sheet (`ASWebAuthenticationSession`), with PKCE (S256). The
  browser comes back to `app.pley.apple:/oauth/callback`, which the sheet
  catches itself: the app registers no URL scheme there (Apple TV's
  registers it for its signed Top Shelf links). The answer must carry the
  `state` the app sent and the server's issuer as `iss`. The sheet is a
  private session, so no cookie is shared with Safari or left behind: the
  password is typed at every sign-in, unless Settings → Sign in privately
  is turned off.
- **Apple TV** shows a code, `BCDF-GHJK`, and the server's `/link` address,
  where it is typed on a phone or computer; the page must be on the
  server's own address. The app asks the server every 5 seconds, 5 seconds
  more for good after each `slow_down`, until the code is allowed, refused
  or runs out (after 10 minutes).
  iPhone and iPad can sign in with a code too. The Mac can't: the server
  refuses it codes.

A token answer that allows more than the app asked for is revoked at once
and refused. Signing in again to a server ends the sign-in it replaces,
when the server proves it holds that sign-in's key (below).

Tokens are kept in the Keychain, one item per server (service
`app.pley.apple.signin`), readable after the first unlock and on this
device only: never synchronised or restored to another device. On the Mac
they go to the data protection keychain; a build signed to run locally
without a team uses the login keychain. The list of servers is kept in the
Keychain the same way (`app.pley.apple.servers`).

Access tokens last an hour and are refreshed a minute before they end, or
when the server refuses one. One refresh runs at a time, and the new
refresh token is kept before the new access token is used. The app is
signed out, and shown sign-in again, when:

- the server refuses a refresh (`invalid_grant`), or refuses a token
  moments after the refresh that gave it;
- a refresh allows more than the sign-in had;
- a request with a token reached an address that isn't private over plain
  http ([Plain HTTP and HTTPS](#plain-http-and-https)).

Signing out revokes the sign-in on the server (best effort, and only
where the server proves itself) and deletes the tokens.

### Telling servers apart

An address the app saved can later lead to another device: on another
network, or after the router hands it out again. Before a token goes to
the server's address, the server there proves it is the one signed in to
([API.md](API.md#telling-servers-apart)). The app keeps the server's id
and this sign-in's key from its first token answer, with the tokens in the
Keychain, and never takes another for that sign-in: a refresh or a
profile switch whose answer brings a different key keeps the first. A key
written wrong (a `proof_id` that isn't digits, a key that isn't 32 bytes
in base64url) counts as none.

To check, the app asks `GET /api/server?challenge=…&proof_id=…` with 16
random bytes in base64url, follows no redirect, and compares the answer's
`id`, and its `proof` against its own HMAC-SHA256 under the key, in
constant time. The proof covers the address the app asked at.

- **The access token** waits for a proof made within the half hour on
  this network. Any change of the network's path (`NWPathMonitor`: joining
  or leaving a Wi-Fi or Ethernet network, another gateway) asks for a new
  one, and so does a request to the server that got no answer.
- **A refresh, a profile switch's PIN and a sign-out** each wait for a
  proof made then. A sign-out whose server can't prove itself deletes the
  tokens on the device only: the person can end the sign-in in the web
  app's Account → Apps.
- **Downloads** carry the access token in each piece's request, so a piece
  waits for a proof as any request does. A piece that goes on from where
  it stopped (its resume data holds the token it began with) waits for one
  too. When the network changes while a piece is under way, or the app
  starts again with one under way, the piece is stopped where it is and
  goes on after a proof: the system would otherwise carry it on over
  whatever network comes next. While the app is suspended or closed, the
  system goes on with a piece it started without the app knowing that the
  network changed.
- **Players' keyed addresses** carry a key of their own, not the token
  ([API.md](API.md#keyed-urls)): it opens one playback session's media and
  comes from a play request that went after a proof, so the players' media
  requests need none of their own. A keyed request the app makes itself
  (a stream's playlists asked ahead of the player) that gets no answer asks
  for a new proof all the same.
- **Top Shelf** holds no sign-in: it shows what the app saved, and the app
  fetches its pictures with the token, after a proof.

A wrong `id` or `proof`, a redirect, a `403` or any other answer means
another device answers there: no token goes, the app shows what it saved
and the banner says "Another device answers there. Showing what Pley had
saved. Reconnecting…", and tries again as it does for a server out of
reach. A `5xx` is the server, or a proxy before it, out of reach for now.

A sign-in kept before servers gave keys has nothing to check: its tokens
go as before, as on Android, until a refresh brings the sign-in's key,
which is kept from then on.

## Profiles

A sign-in acts as one profile of the household. The app lists the
household (`GET /api/oauth/profiles`) and moves the sign-in to another
profile with Pley's switch-profile grant, sending the profile's PIN when it
has one. A PIN is 4 to 8 digits, checked before it is sent. On success the
tokens are replaced, as the old ones stop at once; a refusal changes
nothing. The server's reasons are shown in words: a wrong PIN, too many
wrong PINs lately, a PIN locked after too many, a profile that needs its
password, or one that can't be used from the app.

A profile that needs its password is chosen by signing in again: on
iPhone, iPad and Mac in the browser, where the person picks it; on Apple TV
with a new code, picking it on the consent page.

## What each device plays

Before playing, the apps tell the server what the device plays: a version 2
caps report ([API.md, Version 2](API.md#version-2)), the same kind the
Android app sends. The server decides from it whether a file plays as it
is, is repackaged into HLS, or is converted. The report is built from what
the system says and from a table of Apple's devices (`PleyCaps`):

- **Containers**: MP4, M4V, MOV, M4A, MP3, FLAC and WAV play as they are.
  AVPlayer opens no Matroska, so an MKV file is repackaged into HLS, its
  video copied, whenever the device decodes it.
- **Video**: H.264 on every device; HEVC where VideoToolbox decodes it in
  hardware; AV1 where it does too (A17 Pro, A18 and later, M3 and later);
  VP9 only where its hardware decoder is registered (macOS, and iOS and
  tvOS 26.2 and later). The sizes, frame rates, levels and 10-bit come from
  the device's class, since VideoToolbox only says whether a codec is
  decoded in hardware:

  | Class | Devices | H.264 | HEVC | AV1 | Dolby Vision |
  |---|---|---|---|---|---|
  | A8 | Apple TV HD | 1080p60 | no | no | no |
  | A9 | iPhone 6s and SE, iPad 5th generation, the first iPad Pros | 4K30 | 4K30, 8-bit only | no | no |
  | A10 | iPhone 7, iPad 6th and 7th generation | 4K30 | 4K30, 10-bit | no | level 7 |
  | A10X | iPad Pro 2nd generation, Apple TV 4K 1st generation | 4K60 | 4K60, 10-bit | no | level 9 |
  | A11 to A16, M1, M2 | iPhone 8 to 15, the iPads of those years, Apple TV 4K 2nd and 3rd generation, the first Apple silicon Macs | 4K60 | 4K60, 10-bit | no | level 9 |
  | A17 Pro and later, M3 and later | iPhone 15 Pro and later, iPad mini A17 Pro, iPads with M3 or later | 4K60 | 4K60, 10-bit | 4K60, 10-bit | level 9 |
  | Intel | Intel Macs | 4K60 | 4K60, 10-bit, where VideoToolbox decodes it | no | no |

  Devices are matched by model identifier (`iPhone15,2`, `AppleTV14,1`),
  Macs by chip. A device the table doesn't know takes the newest class when
  its model number (a Mac's chip generation) is newer than any the table
  has, else the A11 to A16 class's.
  VideoToolbox can always take a codec away, never add one.
- **HDR**: reported only when AVPlayer says the device is eligible for HDR
  playback (`eligibleForHDRPlayback`), since an HDR-only stream doesn't open
  on an SDR screen, and only with a 10-bit HEVC decoder. iPhone, iPad and
  Apple TV report the HDR modes the system lists (HDR10, HLG, Dolby
  Vision), which on Apple TV are the TV's. The Mac reports HDR10 and HLG
  when the screen its window is on shows more than SDR's white (an XDR or
  HDR display), and Dolby Vision too on Apple silicon. HDR10+ is never
  reported: AVPlayer shows it as HDR10.
- **Dolby Vision**: with a Dolby Vision screen, profiles 5 and 8 at the
  class's level. Never profile 7, whose two layers AVPlayer doesn't decode:
  the server makes it 8.1 for these apps, or plays its HDR10 base layer.
- **Audio**: AAC, MP3, ALAC, FLAC, Opus, AC-3 and E-AC-3 are decoded on the
  device (Opus from iOS and iPadOS 17; iPadOS 16 gets it converted). A
  stream copies only AAC, MP3, AC-3 and E-AC-3, so the server converts
  Opus and FLAC to AAC when it repackages a file. Nothing is passed
  through, since the system decodes everything and sends PCM (or Dolby
  Atmos in E-AC-3 on Apple TV) to a receiver.
  TrueHD and DTS are converted by the server, to E-AC-3 5.1 when the output
  takes surround sound. The channels are the output's: 6 with spatial
  audio (AirPods), 2 for Bluetooth, headphones and built-in speakers, what
  HDMI reports on Apple TV (2, 6 or 8), and the default output device's on
  the Mac.
- **Subtitles**: the app draws them all (`subtitle_codecs`): text
  subtitles (SRT, ASS/SSA, WebVTT, MP4 text) and picture subtitles (PGS,
  VobSub, DVB), which it reads from the server's `.mks` of the track
  (`picture_subtitles_sideload`), so they never force a conversion.
- **HLS**: master playlists, a stream's several sizes (AVPlayer moves
  between them), and fMP4 segments only, which the server sends these
  apps for every HLS stream ([API.md, fMP4 segments](API.md#fmp4-segments)):
  Apple's players take HEVC, HDR and Dolby Vision only from fMP4. From a
  transcoder that makes no fMP4, the server sends MPEG-TS anyway, with
  HEVC converted to H.264, the one video codec Apple's players take from
  it.
- **Screen and device**: the screen's size in pixels and refresh rate (on
  Apple TV with Match Content's frame rate on, the film and TV rates it
  switches to); the device's form (`phone`, `tablet`, `tv`, `desktop` for
  the Mac), its model identifier and a tier (low, mid or high) from its
  class and memory.

The report is read once and again when something it depends on changes:
the HDR the system reports (a TV switched, a display connected), the
audio route, or on the Mac the screen the player's window moves to. A new
report applies to the next playback session. On the Mac, an HDR video whose
window moves to a screen that shows HDR differently (an XDR display and an
SDR one) starts a new session where it is, beside the one playing as a
quality change does, so it plays as that screen shows it.

What a device reports can be seen without playing anything: on a Mac,
`swift run --package-path apps/apple/PleyKit pley-apple-check --print-caps`
prints what its probe read and the report built from it; a debug build of
the apps logs the same once, under the subsystem `app.pley.apple`, category
`caps`. Neither holds personal data.

## Playing

The player is `PlaybackController` (`PleyPlayer`), a port of the Android
app's session player, over AVPlayer. It plays one item, at the quality
picked in its Quality menu ([Quality](#quality)); at the Original, the
server picks a file's way to play from the caps report (`mode: auto`).

**Starting.** The item's details and the device's caps report are read
together. The file is the one asked for, else the one the device plays
best (the Android app's rule: as it is before repackaged before converted,
then one that fits the screen, the largest, the best dynamic range and
the highest bitrate), and it starts at the resume point unless asked to start
over. `POST /api/playback` asks for keyed addresses (`keyed_urls: true`)
and names the audio or subtitle track only where the server needs it. A
repackaged stream whose audio isn't the one chosen is asked for again with
the track named. AVPlayer then opens the keyed address the answer gives,
at the start position exactly.

**Reports.** `POST /api/items/{id}/progress` with the session, on the web
and Android players' cadence:

- `playing` at the first frame, then every 10 seconds while playing (or
  buffering to play);
- `paused` at once on a pause, then every 30 seconds while paused;
- one second after a seek, so the dashboard follows;
- `stopped` at the end, then `DELETE /api/playback/{id}`, which ends any
  conversion.

Reports go out one after the other, in the order they were made, on a
queue that outlives the player, so a player that closes still sends its
last position and ends its session. A new session (a fallback, an expired
session started again) ends the one before first; a quality change
prepares the next one first ([Quality](#quality)). Closing waits for the
reports at most 2 seconds.

**Answers.** A report answered `410` (`session_stopped`) means the session
was stopped from elsewhere: the player stops, keeps the place with one
last `stopped` report, deletes nothing, and says "Playback was stopped by
the server's administrator", with the administrator's message, without
Try again. The server's `session_stopped` event usually comes first and
closes the player ([Updates from the server](#updates-from-the-server));
the report's `410` is for when the event stream isn't connected. The
media of a stopped session answers `410` too; the player
asks the session's status before taking that for an expiry. A report
answered `404` means the session expired (90 seconds without one): the
next play or seek starts a new session where it was.

**Reports that can't go out.** A `paused` or `stopped` report the server
can't be reached for is kept on the device, in a file per server and
profile in `Application Support/Pley` (the item, the position, the state
and when it was made: never a token or a session). The newest per item
is kept, for 100 items and 90 days. They are sent with their `age_ms`, so
the server keeps whatever happened to the item since, at the next
playback on that server and profile.

**When it fails.** As on Android:

- A file AVPlayer can't open as it is gets a repackaged stream, then a
  converted one; a video it can't decode goes straight to a conversion.
- An expired session or a broken stream is started again where it was,
  three times, and the count starts over after 5 seconds of playback.
- Stalled for 12 seconds with data loaded (decoding too slow), a file
  played as it is gets a conversion. A stream that stalls is checked on
  the server: a conversion slower than real time suggests a lower
  quality, else the session starts again, and stuck twice it stops.
- A repackaged stream whose time runs on for 5 seconds without a picture
  (video AVPlayer opened but can't show) gets a conversion, once.
- A file played as it is that drops frames (more than 1 % of them over
  10 seconds, or 10 at once) gets the server's conversion, once.
- The server out of reach while it plays (no answer, or a gateway in
  front of it answering `502` or `504`): AVPlayer plays what it has
  loaded, and the new session is asked for again, 0.5 to 2 seconds apart,
  for 90 seconds from when the server was lost, as long as the server
  keeps a session it doesn't hear from. "Reconnecting…" shows once
  (iPhone, iPad and Mac); once the server answers, it plays on where it
  stood, and after 90 seconds it says the server can't be reached, with
  Try again. A server that answers, even with an error, isn't waited for,
  and neither is one out of reach before anything played.

AVPlayer decodes video with or without a view on screen: a video output
is attached to every item, and the frames it gives are counted for that
last rule.

**Leaving the app.** On iPhone, iPad and Apple TV, going to the background
pauses and reports at once. Back after more than a minute (time the
device slept counts), the session is checked, and one the server forgot starts again where it was, paused.

### Resume, skips, Up Next and chapters

**Resume.** An item left part way starts where the profile left off,
exactly (AVPlayer seeks with no tolerance before it plays; a conversion
starts producing there). Watched items, and Play from start, start at 0.

**Skip Intro and Skip Credits** come from the item's markers for the file
playing (the server finds them from chapters, shared audio or the
credits' pictures: [Intro and credits
detection](API.md#intro-and-credits-detection)). The button shows
from a segment's start until a second before its end. Skip Intro lands
750 ms before the intro ends (or half-way into a very short one), so its
last moment plays; Skip Credits goes to the credits' end. Nothing is
skipped by itself.

**Up Next** follows an episode with the next one
(`GET /api/items/{id}/next`, looked up at 90 % of the episode, or when its
credits begin). Entering the credits brings up a card, once per segment
(seeking back out of them hides it, and coming back into the same credits
doesn't bring it again); the end of the episode brings it up too, also
after it was cancelled. The card shows the next episode's picture, "Up
next · S1 E2", its title, and counts 10 seconds down while it plays (the
countdown waits while paused), then plays it; Play Next plays it at once,
Cancel hides the card. With Play the next episode automatically off
(Settings), the card shows without its countdown and nothing plays by
itself. Playing the next one ends this session first
(`stopped` where it got to, then `DELETE`: at 90 % or more the server
marks it watched), then starts the next one on the same player from its
resume point, or from the start if it was watched, with the quality chosen
(Automatic starting from what the server knows of the network). Movies
have none; Play Next on the last episode says "This is the last episode".

A collection or playlist plays as a **queue**: the request carries the
queue's titles in order (at most 5,000) and where it is, and Up Next
follows each title with the next in the queue, movies included, read
fresh from the server (`GET /api/items/{id}`) at the same moment. Its
card says "Up next" with the title. At the queue's end nothing plays
next.

**Chapters** from the file are listed with their start times, the one
playing marked, and jump there. On Apple TV they are the player's chapter
markers, each with its picture (`/api/files/{id}/chapters/{n}.jpg`, JPEG,
read with the token; a chapter without one is shown without).

### Speed, sleep timer and Picture in Picture

**Speed** plays at 0.5×, 0.75×, 1×, 1.25×, 1.5× or 2×, the voices' pitch
kept (`AVAudioTimePitchAlgorithm.spectral`). It holds for the title
playing, also when its session starts again, and the next title starts
at 1×. The system player's own speed control is hidden, so there is one.
Each device reports its position as it is; the server sees the rate in
the playback's state.

**The sleep timer** is music's ([Music](#music)): Off, 15, 30, 45, 60 or
90 minutes, or the end of this episode or movie. It counts while it
plays, the minutes going by at the speed playing. In its last 30 seconds
"Pausing in 30 s" shows with Keep watching, which adds the same time
again (or waits for the next episode's end); on Apple TV, Keep Watching
is the player's first contextual action and the seconds show at the top.
Then it pauses. Ending with the episode, Up Next's card shows without its
countdown, and nothing plays by itself.

**Picture in Picture** plays the video in a small window over other apps
(iPhone, iPad) or over Pley (Apple TV):

- **iPhone and iPad**: from the player's own button, and by itself when
  Pley is left while a video plays. The player screen goes away and the
  app shows behind; the playback and its session go on. The window's
  restore button brings the player back, where it was, and its ✕ ends
  the playback, as closing the player does. Leaving the app without
  Picture in Picture pauses, as before.
- **Apple TV**: from the transport bar's button; Pley shows behind, and
  the window's buttons bring the player back or end the playback.
- **Mac**: from the controls' button; the window stays open, and
  restoring brings it to the front.

Starting another video, or signing out, ends the one in Picture in
Picture first. `PictureInPictureKeeper` holds the player's
`AVPlayerViewController`, its subtitles inside, while SwiftUI's player
screen is away.

### Audio

The server says which audio track plays (`audio_index`): the one the
profile picked for the movie or show, else the best of the profile's
language for this device. The Audio menu lists the file's tracks
("English (AAC Stereo)", "English – Commentary (AAC Stereo)") with that one
marked. Picking another saves it for the whole show, or the movie, with
the subtitles as they are (`PUT /api/items/{id}/streams`), and:

- a file played as it is (an MP4 or MOV, whose tracks AVPlayer switches)
  switches to it in the same session, when the device decodes it. A
  track is the option at its place in AVPlayer's audio group (the file's
  track order is ffprobe's), checked against its language;
- otherwise a new session starts beside the one playing with the track
  (`audio_index`, `keep_previous`), as a quality change does.

A track chosen in AVKit's own audio menu (a file played as it is, with
several tracks) is saved the same way. A file played as it is starts with
the track the server named.

### Subtitles

The subtitles shown are the ones the server names for the item
(`selected_subtitle`: the profile's choice for the movie or show, else
what its subtitle setting picks). The Subtitles menu lists Off and the
file's tracks ("English (SRT)", "French (PGS, Forced)", "English (SRT,
External)" for a file beside the video or one added on the server).
Picking one saves it for the whole show, or the movie
(`PUT /api/items/{id}/streams`, with the audio only when one was picked),
and shows it at once, in the same session.

The app draws every track itself, over the video, whatever the session
plays (`PleySubtitles`):

- **Text** comes from the server's WebVTT
  (`/api/files/{id}/subtitles/{index}.vtt`, 16 MiB at most, read with the
  token). Italics, bold and underlining are kept; a cue placed in the
  upper half goes at the top.
- **Pictures** (PGS, VobSub, DVB) come from the server's `.mks` of the
  track (`/api/files/{id}/subtitles/{index}.mks`, 512 MiB at most),
  downloaded into the app's caches and deleted when the player closes.
  The app reads it with its own Matroska reader and decoders, ports of the
  web app's (`packages/picture-subtitles`), and draws each picture where
  its plane puts it, scaled to the video's bounds. A session asks for no
  `subtitle_index`, so nothing is burned in and a file that plays as it
  is, or repackaged, stays so.
- The pictures are decoded away from the main actor, so a heavy one
  never holds up the screen or the remote: the picture at the playhead
  and the next one, decoded ahead, so each is ready by its time. After a
  seek or another track, a picture shows once it is decoded. The pictures
  near the playhead are kept decoded, and stay so while the track grows.
- While the server is still reading a track out of its file, its answer
  is partial (`X-Pley-Complete: 0`): what came shows, and the rest is
  asked for every 10 seconds (the `.mks` from where it stopped, with
  `Range`), until the answer is whole.
- A server that can't make the `.mks` (`404` for one without the route,
  `503` when its transcoder can't) has the stream start again with the
  pictures burned in (`subtitle_index`), as before; Details then says
  they are burned in, and they can't be moved in time.

The text is white and bold with a dark outline and no box, 4.8 % of the
video's height and 6 % above its bottom, as on the web and Android. When
a caption style is chosen in the system's settings (Accessibility →
Subtitles & Captioning), what it sets wins: colours, background, font,
size and edges; a change there shows at once.

The app's own Subtitles settings then apply on top, each left at System
changing nothing: Size scales the text (75 % to 200 %), Background sets
none or a dark box (black at 75 %), Font a family (sans serif, serif,
monospaced, or the casual Noteworthy where the device has it) and
Position raises the text cues 5, 10 or 15 % of the video's height.
Picture subtitles keep their size and place, as their plane sets them. A
change shows at once, also over a video playing.

The player's **Subtitle style** menu has the same four, each with its
choice beside it, and keeps a change for every video, as Settings does.
As on Android, it shows only while text subtitles do: picture subtitles
keep their own look, and Off has nothing to style.

The subtitles are looked at as often as they change, 20 to 250 ms apart
while it plays and every 500 ms while paused. **Subtitle sync** moves
them 0.1 s at a time, up to 10 s earlier or later (positive: later), and
the offset is kept on the device for the file and the profile, under the
server's address (`UserDefaults`, never on the server).

**Find Subtitles…**, last in the Subtitles menu while a title streams
from the server, searches online through the server
(`GET /api/items/{id}/subtitles/search`, the file playing and the
language), in a sheet on iPhone, iPad and Mac and a list over the player
on Apple TV. It searches in the profile's subtitle language (else its
audio language, else the server's default), and Language picks another.
Each result says its name, its language, Hearing impaired, Forced, Made
for this file (matched by the file's hash) and its downloads, best first.
Choosing one has the server add it to the title
(`POST /api/items/{id}/subtitles`): the new track joins the Subtitles
menu, shows at once, and "Subtitles added" says so; the search closes.
A server without subtitle search set up says "Subtitle search isn't set
up on this server" (`503` with `setup: false`); a refused key or the
day's downloads used up gives the server's own words, with no Try again;
a server without the routes says it may need an update. The playback
goes on meanwhile.

### Quality

The Quality menu is the one in [QUALITY.md](QUALITY.md#the-menu): Original,
Automatic and each quality that makes the file smaller, with Original's
size and bitrate and Automatic's "Now 8 Mbps 1080p · Lowered for your
connection" as second lines. The choice is remembered on the device for
each kind of network, cellular and the rest (`NWPathMonitor`), and the next
playback on that kind of network starts with it. Automatic is the default.
Settings' Quality sets the same choices (Quality on Wi-Fi and on mobile
data, on a device with mobile data), so a choice made in either place
shows in the other.

**Automatic** follows [QUALITY.md](QUALITY.md#automatic), with the Android
app's numbers (`AutoQuality`):

- It asks the server what it knows of the network
  (`GET /api/playback/network`, 1.5 seconds at most, alongside the item's
  details) and, on a network the server asks to measure, reads
  `GET /api/playback/speedtest` for 2 seconds from its first byte and sends
  the result back; all of it takes 3.5 seconds at most. The server's own
  network is never measured. Its later ceilings go to the server behind the
  reports.
- When the server can't say, the device's own memory decides: the ceiling
  at the last step down, kept per server for 30 minutes on the time since
  the device started, time asleep included (`UserDefaults`), so a restart
  forgets it.
- It watches the waits, the seconds of video loaded ahead and AVPlayer's
  own measure of the network (`observedBitrate` in the access log), every
  half second, and the dropped frames (the access log's count against the
  seconds watched and the video's frame rate) every 2 seconds.
- On a stream in several sizes, AVPlayer moves between them, and plays
  none bigger than the player on screen (`preferredMaximumResolution`, in
  pixels); a quality picked plays its own size.

**Changing the quality.** A quality picked, or Automatic's step, starts the
next session beside the one playing (`keep_previous`) and asks for its
playlist and the segment it starts with as AVPlayer will, 20 seconds at
most, so the transcoder has it ready. The player then opens the new stream
from where the old one got to, at that position exactly, and the old
session is deleted without a `stopped` report, which would only move the
resume point back. A second change while the first gets ready wins, and
the first session is deleted. With no transcode free for both (`503`), the
old session ends first. Automatic's step down says "Quality lowered to
4 Mbps 720p" once; going back up is silent. Picking Original or Automatic
while the original plays changes nothing on the screen.

**Details** adds the quality playing, why Automatic lowered it, and, for a
conversion, what the server makes of the video ("H.264 1280×720"), read
from the session's status once it plays.

### Music

Music plays apart from the player, as on Android and the web: one
`MusicPlayback` (`PleyPlayer`) for the app's life, so it plays on with no
page open, with the screen locked and in the background (the `audio`
background mode). It is a port of the Android app's `MusicPlayback`:

- **A session per track**, asked for as a video's is (`POST
  /api/playback`, `mode: auto`, keyed addresses, the caps report), with
  music's own client id (the device's with `-music` after it), so a video
  starting doesn't end music's session, nor the other way round. Before
  the next track starts, the one before ends (`stopped`, then `DELETE`,
  waiting 2 seconds at most), so a conversion never waits on it.
- **Reports**: progress every 10 seconds while it plays and at a pause; at
  a track's end, `stopped` at its length, and the next track starts.
  Reports that can't go out wait with the video's ([Reports](#playing)).
- **Failures**: a track the device can't decode is asked for again as a
  conversion, from where it was; an expired session (`404`, after a long
  pause) or a network error starts a new one where it was, three times at
  most; the server stopping it (`session_stopped`, or `410`) stops the
  music, says "Playback was stopped by the server.", and Play starts the
  track again. Otherwise it says "This track can't be played." or "Can't
  reach the server."
- **Previous** starts the track again after 3 seconds of it, and goes to
  the track before otherwise. **Shuffle** keeps the track playing first
  and plays the rest in a random order; off, the order given comes back
  at the same track.
- **The sleep timer**: Off, 15, 30, 45, 60 or 90 minutes, or the end of
  this track (the next one waits, paused). Its menu says what is left
  ("15 min left"); in its last 30 seconds, "Pausing in 12 s" shows with
  **Keep listening**, which adds the same time again. It isn't kept
  between playbacks.
- A video or Live TV starting pauses the music, which waits where it was;
  music starting closes the video or Live TV playing. Switching profile,
  signing out or opening another server stops the music.

**The mini player** (iPhone, iPad and Mac) shows the album art, the title
and the artist, Play/Pause and Next, and a thin line of how far the track
is. VoiceOver reads it as one element ("Now playing, Sunson, Nils Frahm")
with Play, Pause, Next and Open as its actions; at the accessibility text
sizes its words and buttons are on lines of their own. Choosing it opens
**Now Playing**: a sheet on iPhone and iPad, a window of its own on the
Mac (also in the Window menu, ⌥⌘P). Now Playing has the album art (600
pixels at most), the title, the artist and the album (each opening its
page), a scrubber with the time played and left (VoiceOver moves it 10
seconds at a time), Previous, Play/Pause and Next, Shuffle, the sleep
timer, AirPlay, and Up next, where choosing a track plays it. On **Apple
TV**, the Now Playing tab holds the same, with Up next as a row of
cards; the tab comes when music starts (and opens) and goes when the
queue is cleared. An album's page there opens with the focus on Play.

**Now Playing on the system and the remote commands.** One owner at a
time (`NowPlayingCenter`): whichever of music, a video and Live TV played
last fills `MPNowPlayingInfoCenter` (the lock screen, Control Center, the
Mac's menu bar and media keys, a headset's buttons) and answers
`MPRemoteCommandCenter`'s commands, which go through Pley's own
controllers, so each is reported to the server:

| What plays | Commands |
|---|---|
| Music | Play, Pause, Play/Pause, Next Track, Previous Track, a place on the scrubber |
| A video | Play, Pause, Play/Pause, 10 seconds back and forward, a place on the scrubber |
| Live TV | None: it is watched as it airs, as on Android |

AVKit's own Now Playing is turned off on iPhone, iPad and the Mac
(`updatesNowPlayingInfoCenter = false`); Apple TV's player always shows
its own for a video, which tvOS can't turn off, so there music only lets
go of it. When a video or Live TV closes and music has a queue, Now
Playing is music's again. The commands' targets and the artwork's
handler are `@Sendable` closures that MediaPlayer calls on its own
queues; each hops to the main actor before it touches the app.

**The audio session** (`AudioSessionController`, iPhone, iPad and Apple
TV): `.playback` for music, `.playback` with the `.moviePlayback` mode
for a video or Live TV, and let go, with others told, once nothing plays.
A call or an alarm pauses what plays; its end resumes music only when the
system says to. Headphones pulled out pause.

**AirPlay**: music plays on an AirPlay speaker or Apple TV from Now
Playing's AirPlay button (the route picker lists speakers first), or from
Control Center. On Apple TV, AirPlay is the system's.

### The player on each device

Each device shows the system's own player over `PlaybackController`
(`PleyPlayerViews`), so the controls, the scrubber, AirPlay and the
remote's gestures are the ones people know. The system's controls act on
AVPlayer itself; the controller follows them, so a play or pause from the
controls, the Control Center or a headset is reported, and a move of the
scrubber counts as a seek (one after the end starts a new session there).
One playback plays at a time: starting another, signing out, switching
profile or going back to the servers closes the one before, and its
session ends on the server. The player shows the item's title and
episode, and AVPlayer picks no subtitle or audio track of its own: the
audio is the one the server names ([Audio](#audio)), and the subtitles
are drawn by the app in the player's content overlay, under the controls
([Subtitles](#subtitles)).

- **iPhone and iPad**: full screen over the app. The system's Close ends
  the playback. An Options button under the top row (with the controls,
  and while paused) has Quality, Audio (when there are several tracks),
  Subtitles and Subtitle sync (Earlier, Later and Reset, the menu staying
  open while it moves on iOS 16.4 and later), Subtitle style, Find
  Subtitles…, Speed,
  Sleep Timer, Chapters and Details: how
  the file plays (Direct play, Direct stream or Transcoding, and whether
  the audio is converted), why, the video, and the subtitles shown.
  Skip Intro, Skip Credits and the Up Next card show at the bottom right,
  and the sleep timer's last seconds at the top, with Keep watching.
  Picture in Picture is the system's button.
- **Apple TV**: full screen, with Quality, Subtitles, Speed, Sleep Timer
  and Audio in the transport bar (Subtitles holding Subtitle sync, which
  stays open while it moves, Subtitle style and Find Subtitles…) and a Details tab in the info panel (swipe down). Apple TV's
  player doesn't say where the video shows: the subtitles go on the
  picture's size fitted to the screen. Skip Intro, Skip
  Credits, Up Next ("Play Next (8)", Cancel) and the sleep timer's Keep
  Watching are the player's own contextual buttons, and the chapters its chapter markers, with their
  pictures. Menu closes the player when the transport bar isn't showing.
  The display's frame rate and dynamic range follow the video when the
  Apple TV is set to match content.
- **Mac**: a window of its own per playback, opened from the item's page;
  closing the window ends the session, and starting another playback
  closes it. The controls float over the picture, with Quality, Audio,
  Subtitles (Find Subtitles… last), Subtitle Sync, Subtitle Style, Speed,
  Sleep Timer, Chapters and Details in their menu, and Picture in Picture; Skip
  Intro, Skip Credits and the Up Next card at the bottom right, and a
  Playback menu in the menu bar (Play or Pause, Back and Forward 10
  Seconds, Skip, Play Next, Quality, Audio, Subtitles, Find Subtitles…,
  Subtitle Sync, Subtitle Style, Speed, Sleep Timer, Chapters) acts on the window in
  front. The keys are the web player's:
  Space or K plays and pauses, ← or J and → or L go back and forward 10
  seconds, ↑ and ↓ change the volume, M mutes, F goes full screen and Esc
  leaves it, S skips the intro or the credits, and C goes to the next
  subtitles (Off after the last). The Mac going to sleep
  pauses, a move to another screen reads the device's caps again (an
  HDR video starts again for it, [What each device plays](#what-each-device-plays)), and
  quitting waits for the last reports (2 seconds at most).

Quality's choices (and a subtitle track that would be burned in) show
their second line under them: as a subtitle in
the transport bar's menu on Apple TV and in the Mac's menus, as a second
line in the Options menu on iPhone and iPad.

The player logs under the subsystem `app.pley.apple`, category `player`:
the session's method and the first 8 characters of its id, never a keyed
address or a token.

## Downloads and offline

iPhone, iPad and Mac download movies and episodes to watch without the
server, as the Android app does; Apple TV has no downloads. A title's
page has **Download** (a movie or an episode) and, beside a season's
episodes, **Download season**; a toast says "Downloading. It's in
Downloads, under Me." The button then says where the download is
("Preparing on the server · 40%", "Downloading · 62% of 1.4 GB"), and
Downloaded or Try again once it ends. **Me → Downloads** lists the
profile's downloads on this device, newest first, with their pictures,
what they are and where they are ("Waiting for Wi-Fi", "Waiting for your
server", "Didn't download: Not enough space on this device"), and what
they take and what is free; a finished one plays when chosen, and a row
swiped (a right-click on the Mac) is deleted, cancelled or tried again.

**What downloads** (`docs/API.md`, "Downloads"): the app asks the server
(`POST /api/conversions`, with this device's caps report and, below
Original, the quality's height and bitrate) and the server says whether
the file downloads as it is or is made for the device, which the app
follows every 5 seconds while Pley is open, giving up after 15 minutes
without progress. The file comes through a background `URLSession`, so
the system goes on while the app is in the background or closed, and
wakes it when pieces finish. Woken with no window, the app asks for the
next piece as the profile signed in last on the server used last, with
the sign-in kept in the Keychain and without waiting for the server:

- in pieces of 256 MiB, each a `Range` request with an access token that
  lasts at least 10 more minutes, read just before the piece starts; a
  piece is appended to the file in 1 MiB chunks;
- a piece the network broke goes on from where it stopped while its token
  has 10 minutes left, and is otherwise asked for again from the file's
  length; a missing file (`404`) of a conversion is asked of the server
  again;
- one download at a time; before each piece, the space it needs and 500
  MB more must be free, or it stops with "Not enough space on this
  device";
- on an expensive network (mobile data, a phone's hotspot) downloads wait,
  unless Download on mobile data is on;
- plain http follows [Plain HTTP and HTTPS](#plain-http-and-https): a
  piece that came from an address that isn't private is thrown away and
  the download stops; redirects aren't followed; the task carries only
  the download's key, and nothing logged holds an address or a token.

Each download is a folder in Application Support (`Pley/Downloads`, left
out of backups), per server and profile: the file, the title's details,
its picture, and its text subtitles as WebVTT (a track the server is still
reading is asked for again until it is whole). `index.json` holds the
downloads, written whole each time. Signing out keeps them, for when the
profile signs in again; forgetting the server deletes them.

**Playing a download**: the player opens the file on the device instead
of a stream, with no session on the server. Details says Downloaded, the
Quality menu has only that, the subtitles are the text tracks saved with
it, and Find Subtitles… isn't offered. Its reports go without a session
(`POST /api/items/{id}/progress`); one that can't reach the server is
kept, and sent with its age (`age_ms`) once the server answers, so a
newer play elsewhere wins. Where it got to is also kept with the
download, so Resume works offline, and 90 % or more counts as watched.
Up Next is the next episode downloaded.

**Offline**: each screen's last answer is saved, per server and profile
(`Application Support/Pley/Snapshots`, 25 MB at most, the answers read
longest ago going first, the last 3 profiles of each server): Home, the
libraries, a library's first page and Recommended rows, a title with its
seasons and episodes, playlists and a playlist, My List, the first page
of History, the profile, a person, a search and Live TV's lists. When the
server used last doesn't answer at start (3 seconds at most for a home
address) and the app is signed in to it, the app opens as the profile
used last on what it saved, with a banner: "Showing what Pley had saved.
Reconnecting…", with Try again. A server that stops answering while the
app is open does the same. The app tries the server again after 2, 5, 15
and 30 seconds, then every minute, and at once when the network changes
or the app comes back to the front; once it answers, the banner goes and
every screen reads the server again. Apple TV saves screens the same
way, and starts offline only where it would open Home without asking
(one profile, or Always start as naming the profile used last): "Who's
watching?" and a profile's PIN need the server, so otherwise it shows the
servers with why.

## Requests

Where the server takes requests ([API.md](API.md#requests)), people ask
for movies and shows found on TMDB and administrators decide; Radarr and
Sonarr fetch what is approved. The Apple apps follow the Android app on
phones and tablets: its rules, words and timings are ported to
`PleyScreens` and tested on its fixtures. On iPhone, iPad and Mac:

- **Ways in**: Me's rows, Discover ("Find movies and shows to ask for")
  and Your requests (Requests for administrators, "Approve, decline and
  follow what was asked for") with its number; Me's tab carries the
  number, and so does Me's row at the sidebar's foot, in the accent
  colour, and in red while a request couldn't be sent.
  Search shows Discover before anything is typed, and its Discover row
  after the results. The Mac's View menu has Discover (⇧⌘D) and Requests
  (⇧⌘R), which open under Me. An account that may not ask and has no
  requests sees none of it.
- **Search's Discover row**, after the libraries' results: up to 12 of
  TMDB's titles that no library has (`missing=1`, three pages at most),
  read beside the libraries' search without holding it up; with nothing in
  the libraries, "Nothing in your libraries matches" above it. When TMDB
  can't be searched, the row says why, with Retry where trying again may
  help. See all opens every title TMDB finds for the words, with its
  badge, in a grid under "Results for “dune”", read page by page as it
  scrolls. Words past the server's 200 characters are cut to them.
- **Discover**: a hero for the first trending title (when trending has
  nothing the account may ask for, every row is read at once and the hero
  is the first title of the next one with any), with its backdrop, its
  main action (Request, Tell me too, Open or Request more seasons; for
  a title already asked for, its request's stage; with nothing left to
  ask for, a disabled Request and when more may be asked for) and
  Details; rows (Trending this week, Popular movies, Popular shows,
  Coming to cinemas, Popular anime) of posters with their badges ("In
  library", "Requested"), each read as it comes into view and page by
  page as it scrolls; the person's requests, with See all; and TMDB's
  credit. A movie is asked for from the hero; a show opens its page with
  Which seasons?; Stop following and the rest are on the title's page.
  When requests can't be used (off, no access, signed in before them,
  TMDB's key refused or TMDB not answering), the page says why, and only
  that; every row read and none with a title says "Nothing here right
  now".
- **A title's page**, laid out as a library's: the backdrop, poster,
  title, facts and genres; the one main action (Request, Requested,
  Following with Stop following, Tell me too, Open, Request more seasons,
  or the line saying why not) with what the account may still ask for;
  the tagline and overview; the requests for it with their actions; each
  season's state, or its request's stage; the cast; TMDB's credit.
  Request on a show opens **Which seasons?**, a sheet with the missing
  seasons ticked, lowest first, up to what's left; ticking more than
  that disables Request and turns the allowance red.
- **Requests**: views with their counts (Needs attention, oldest first,
  for administrators; All; then the statuses that hold any), Movies or
  Shows, and for administrators a title's words, a person and Yours.
  Each card has the poster, the title and seasons, the stage pill with
  the download's bar, who asked and when, a declined request's reason
  and, for administrators, the note and where it went; then exactly the
  server's actions: Watch once it's ready, Approve, Try again and Request
  again prominent, the rest beside them, and Approve…, Mark as added by
  hand and Delete under More actions. Approve… is a sheet with the
  quality profile, folder and (shows) seasons, starting on the settings'
  choices; Decline takes an optional reason, which the person who asked
  sees; Withdraw, Remove and Delete ask first. Pull, or ⌘R, reads the
  list again; it reads on 30 at a time as it scrolls. Once it shows, the
  person's news is seen and the number goes.
- **A library title** from a request shows its stage under its buttons,
  with the download's bar, and a show with seasons left to ask for
  offers Request more seasons, which opens its page with Which seasons?.
- **News** (ready to watch, approved, declined; for administrators, new
  requests and ones that couldn't be sent) shows as a toast with Open:
  ready to watch opens the title, the rest the list. It never shows over
  the player: what comes meanwhile waits (three at most) and shows once
  the player closes. What came while the app was closed is told once at
  the next start, kept per server and profile.

An app signed in before requests offers **Sign in again** on Discover, a
title's page and the list (and an administrator's app signed in before
approvals, on the list's banner): the server's sign-in shows over the
servers, saying why, and asks for the scopes the server now lists; its
Cancel goes back to the app as it was. The summary follows the event
stream; while the stream is down, the app reads it every minute in the
front, and an open list every 30 s. Download progress sends no events: a
list, a title's page, a library title or Discover's requests showing a
download are read again every 10 s while they show and the app is in
the front.

For VoiceOver, a poster is one element ("Dune: Part Two, 2024, Movie,
Requested") and a request card's title says its stage; numbers on the
ways in are said in words ("3 requests need you"); a download's bar
says its percentage; a page that can't show, and the Discover row's
failure, are read out as they appear. At the accessibility text sizes the
hero's words go under its picture and a card's poster above its words.

Apple TV has no requests' pages: nothing leads to them there, no toast
tells their news, and neither Search nor a title's page reads anything
of requests.

## Top Shelf

With Pley in the top row of Apple TV's Home Screen, the Top Shelf shows
**Continue Watching**: the titles of Home's Continue Watching row, ten at
most, each with its still or backdrop (a poster when it has neither), its
episode ("S2 · E5") and how far through it is. Selecting a title opens
its page in Pley, over Home; Play on the remote plays it. It is Android
TV's Watch Next row, made the same way.

The app writes the row each time Home is read: at sign-in, after a
change on the server (something watched, taken out of Continue
Watching), and back in front. It keeps it in the app group's container
(`group.<bundle ID>`, under `TopShelf/`): the titles, their pictures at
800 pixels as the server sent them, and the key the links are signed
with. Another profile or server replaces the row at once; signing out,
or forgetting the server, empties it, and the shelf goes back to the
app's own picture.

The Top Shelf extension (`PleyTopShelf`) only reads that: it holds no
sign-in and asks the server nothing. Refresh tokens change at every use,
so a second process refreshing them would race the app and could sign it
out, and a token kept in a second process is one more place to take it
from. The shelf therefore shows what the app saw last, until Pley is
opened again.

Each title's links (`app.pley.apple:/shelf/open/<id>` and `…/play/<id>`)
name the server and profile (a hash of the server's address and the
profile's id) and are signed with HMAC-SHA256 under a random key only the
app and its extension can read. The app acts on a link only when the
signature and the server and profile open match it, so another app can't
make one that plays something in Pley. Opened while "Who's watching?"
shows, a link waits, and acts once the same profile is chosen; choosing
another drops it.

## Building and testing

Requirements: a Mac with Xcode 16 or later. The apps use Apple's frameworks only: no Swift package is
downloaded.

Signing lives in `Config/Signing.xcconfig`. Without an Apple account, the
Mac app and simulator builds are signed to run on the building Mac only
("Sign to Run Locally"). Keep them signed: an app built with
`CODE_SIGNING_ALLOWED=NO` can't use the Keychain, so it can't keep a
sign-in and opening any server fails with a Keychain error. Only the
package's tests turn signing off. To
sign with an Apple account, create `Config/Local.xcconfig` (git ignores
it) with your team and a bundle ID of your own:

```
DEVELOPMENT_TEAM = ABCDE12345
PLEY_BUNDLE_ID = com.example.pley
CODE_SIGN_IDENTITY[sdk=macosx*] = Apple Development
```

A Mac build signed to run locally can't use the data protection keychain,
so it keeps its sign-ins in the login keychain, which knows the app by its
signature. That signature changes with every build, so macOS asks for the
login keychain's password after each one ("Always Allow" lasts until the
next build). A build signed with a team, even a free one, is known by its
team and bundle ID instead, which stay the same from build to build, so
one "Always Allow" lasts.

With a free Apple account, apps installed on an iPhone, iPad or Apple TV
stop opening after 7 days and a device takes at most 3 of them; Mac builds
don't expire.

The Apple TV app embeds its Top Shelf extension (the target
`PleyTopShelf`, built by the `PleyTV` scheme), and the two share an app
group, `group.<bundle ID>` (`Config/PleyTV.entitlements`,
`Config/TopShelf.entitlements`). Simulator builds signed to run locally
have it. On a device, App Groups need a team that has them, which a free
account doesn't; such a team leaves the group out by adding two lines to
`Local.xcconfig`:

```
PLEY_TV_ENTITLEMENTS =
PLEY_TOPSHELF_ENTITLEMENTS =
```

Everything else works the same; the app writes no row, and the Top Shelf
shows the app's own picture.

From the repository root:

```sh
# The packages' tests on the Mac
swift test --package-path apps/apple/PleyKit

# The same tests on the simulators (names from `xcrun simctl list devices available`)
IOS_SIM="iPhone 17"; TV_SIM="Apple TV 4K (4th generation)"
(cd apps/apple/PleyKit && xcodebuild test -scheme PleyKit-Package -destination "platform=iOS Simulator,name=$IOS_SIM" CODE_SIGNING_ALLOWED=NO)
(cd apps/apple/PleyKit && xcodebuild test -scheme PleyKit-Package -destination "platform=tvOS Simulator,name=$TV_SIM" CODE_SIGNING_ALLOWED=NO)

# The apps
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley   -destination 'generic/platform=iOS Simulator'  build
xcodebuild -project apps/apple/Pley.xcodeproj -scheme PleyTV -destination 'generic/platform=tvOS Simulator' build
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley   -destination 'platform=macOS' build

# Run the iPhone app on a simulator
xcrun simctl boot "$IOS_SIM"; open -a Simulator
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley -destination "platform=iOS Simulator,name=$IOS_SIM" -derivedDataPath /tmp/pley-dd build
xcrun simctl install booted /tmp/pley-dd/Build/Products/Debug-iphonesimulator/Pley.app
xcrun simctl launch booted com.formandelement.pley
```

The package's tests read the Android app's JSON fixtures
(`apps/android/core/testing/src/main/resources/fixtures`) where they are,
so both apps are tested against the same answers. The network tests talk
to a fixture server the tests start on 127.0.0.1 (two of them where a
second origin is needed), over a real socket, so they need no Pley server
and no network beyond this device.

The caps tests build each recorded device class's report from its probe
(`PleyKit/Tests/PleyCapsTests/Probes`) and compare it with the server's
fixture of the same name (`apps/server/tests/fixtures/caps/apple_<class>_v2.json`),
which the server's tests decide from (`apps/server/tests/playback_apple.rs`):
the report the app sends and the server's answer to it can't drift apart.
After a change to what the apps report, rewrite the fixtures and check the
server's answers:

```sh
PLEY_UPDATE_CAPS_FIXTURES=1 swift test --package-path apps/apple/PleyKit --filter RecordedDevicesTests
cargo test -p pley-server --test playback_apple
```

The recorded probes follow Apple's published specifications; a device's
own reading, from `--print-caps` or a debug build's log, replaces its
class's when one is recorded.

One test finds a server by Bonjour for real: a fixture server announces
`_pley._tcp` on the Mac's loopback interface, and `LocalServers` must find
and confirm it. It needs mDNSResponder and local network access, so it
runs only when asked:

```sh
PLEY_BONJOUR_TEST=1 swift test --package-path apps/apple/PleyKit --filter aServerAnnouncedByBonjourIsFound
```

`dns-sd -B _pley._tcp` lists the servers announced on the network. A
server bound only to 127.0.0.1 announces nothing: announcements go out on
the adapters with addresses on the local network.

### Icons

`bun scripts/render-icons.mjs` at the repository root draws the apps'
icons from the logo, as it does the other apps', into the asset catalogs
each target's `ASSETCATALOG_COMPILER_APPICON_NAME` names:

- `Pley/Assets.xcassets`, `AppIcon`: one 1024 px picture for iPhone and
  iPad, full bleed and opaque, which the system rounds (the web app's
  `apple-touch-icon.png`, larger), and the Mac's sizes on Apple's grid,
  as the desktop app's `icon.icns`.
- `tvOS/Assets.xcassets`, `App Icon & Top Shelf Image`: Apple TV's icon
  in two layers, the tile's colour behind and the mark in front, which
  part as it's focused, at 400×240 (800×480 at 2×) and the App Store's
  1280×768; and the Top Shelf's own picture, the iPhone icon beside
  Pley's name at 1920×720 and 2320×720, shown while the app has no row there
  ([Top Shelf](#top-shelf)).

### UI tests

Two UI-test targets drive the apps on the simulators: `PleyUITests`
(`UITests/`, iPhone and iPad) and `PleyTVUITests` (`TVUITests/`, Apple TV,
with the remote), each with a shared scheme of its name. They walk the
screens as a person does, by the views' accessibility identifiers, which
read `<screen>.<part>[.<id>]` (`servers.address`, `signIn.userCode`,
`profiles.profile.Kids`, `home.row.recently_added:1`, `tile.item.42`,
`me.signOut`):

1. Add the server by its address and sign in with a code, which the test
   allows as the administrator through the consent page's endpoints.
2. "Who's watching?": Kids, by its PIN.
3. Home with its tiles; an item's page with Play, and back.
4. Me (Settings on Apple TV) says Kids; Switch profile back to the
   administrator, by its PIN.
5. See all on Home opens the library sorted newest first.
6. A movie's menu: Mark as watched, and on iPhone and iPad back to
   unwatched; Add to My List, and on iPhone and iPad out again; each
   checked on the server. On Apple TV, then: along Home's movies row,
   down a row and back up, the focus is on the tile it left; and
   Play/Pause on a movie's tile opens the player, which Menu closes.
7. Movies: sort by year, the other way, Unwatched, and Clear.
8. Save as Smart Playlist on Movies: the smart playlist's page opens with
   no way to change it, and the server has its filter; the test deletes
   it.
9. Movies' Collections tab: "Test collection"'s page, with Play.
10. A playlist: a movie's menu, Add to playlist and a new playlist "UI
    Test"; a second movie added to it from its menu; on its page (from
    Playlists) the second entry moved up; a removal undone, which never
    reaches the server; a removal, gone on the server after 5 seconds;
    Rename and Delete. Each step is checked on the server.
11. Clips (40 titles): down the grid, then C goes back to Clip 01.
12. On iPhone and iPad, Edit and a drag put the first library last; the
    server has the new order, and the test puts the server's back.
13. Search: "clip" finds Movies with their count; a movie found opens,
    and with the field emptied "clip" is the first recent search.
14. Me's Subtitles set to Always, checked on the server (the test puts it
    back); History shows.
15. Settings: Quality on Wi-Fi to 4 Mbps 720p and subtitles at 150 %;
    Diagnostics shows the caps report, and About the version.
16. On Apple TV, relaunched: "Who's watching?" shows, and going on as the
    administrator, the profile in use, asks for its PIN; with Always start as
    the administrator, a second relaunch opens signed in as them, without
    asking (the test puts it back to asking).
17. Sign out, confirmed; the servers show, and the server no longer lists
    the app among the administrator's signed-in apps.

Between steps 6 and 7, the title pages:

- A movie (Two Audio, given a genre it shares with Direct H264 by an
  administrator's edit): its French track picked in Audio and four stars
  given, each checked on the server; More Like This and Media info show;
  Play opens the player, which the page behind can't be reached through,
  and closing it comes back to the page.
- Shows: the show's page with its episodes, its season's page with its
  three episodes, and an episode's page.
- Music: the artist's page with Play, and the album's with its two tracks,
  Play and Shuffle.

On iPhone and iPad, a test of its own looks at the screens at the largest
accessibility text size (Accessibility XXXL), with music playing so the
mini player shows under them: the album playing, Now Playing, Home, a
movie's page and below it, Movies, a show with its episodes, "Test
collection", Playlists, Search, Me, Settings and About, each with its main
control checked to be on screen and reachable. Its screenshots are named `large-*`. It changes
nothing on the server but music's session, which signing out ends.

Music is a test of its own (`testMusic`): the album's Play starts the
first track (the mini player on iPhone and iPad, the Now Playing tab on
Apple TV), the track playing is marked, and the dashboard shows its
session; Next plays the second, whose session takes the first's place;
Now Playing turns Shuffle on and the sleep timer to 15 minutes ("15 min
left"); a music playlist of the artist's tracks (made from Add to
playlist on iPhone and iPad, through the server on Apple TV) plays from
its second entry, and Pause (the remote's Play/Pause on Apple TV) pauses
it. The playlist is deleted afterwards, and signing out ends the
session.

Downloads are a test of their own on iPhone and iPad (`testDownloads`):
the H.264 MP4's Download turns into Downloaded within 90 seconds; Me →
Downloads lists it, and it plays from there with Details saying
Downloaded and no session on the server; the show's Download season has
the server make its episodes for the device, and they all download; and
Settings → Downloads → Delete all downloads leaves Downloads empty.

Live TV is a test of its own (`testLiveTv`), against `live-source.sh`'s
four channels with one connection: All channels shows the guide with Test
One's programmes; a channel plays ("playing" in the player); channel up
tunes Test Two (on Apple TV, up on the remote, then 1, 0, 3 and OK on the
number pad tune Test Three); the star makes the channel a favourite on
the server; leaving ends the session at the server within 4 seconds;
Favourites and Recently watched list the channels; with another device
holding the connection, the player says "All connections are in use" and
Watch here instead takes it, the other device's playlist answering 410;
and Search finds the channels.

The Top Shelf is a test of its own on Apple TV (`testTopShelfLink`):
with Two Audio part way through, the app (launched with `-PleyShelfLinks
YES`, which in a debug build shows the first title's signed links for the
test to read) saves it first in the row. Each link is opened as the shelf
opens it, which starts the app again at "Who's watching?", where the
link waits until the administrator is chosen: the same link unsigned, or
with another signature, leaves Home as it was; the signed `open` link
shows Two Audio's page, and the `play` link plays it, its session on the
server. Signing out empties the row. The shelf itself, on the Home Screen, is
only seen by hand: put Pley in the top row of the simulator's Home Screen.

The player's extras are a test of their own (`testPlayerExtras`), on
Direct H264: Find Subtitles… (in Options on iPhone and iPad, the
transport bar's Subtitles on Apple TV) says "Subtitle search isn't set up
on this server", as the test server has none; Speed 1.5× is marked once
picked; and where the simulator has Picture in Picture
(`AVPictureInPictureController.isPictureInPictureSupported()`), its
button shows the page behind with the session still on the server, and
playing again ends it. The iPad simulator has Picture in Picture; the
iPhone and Apple TV simulators have none, so that part is passed over
there.

A person's page is a test of its own, which opens the first title with
cast; the test server has no metadata source, so no title has cast, and
the test reports itself skipped.

They need a server set up for them, so a script runs them:

```sh
apps/apple/scripts/check-screens.sh
```

It builds the server, makes the screens' media
(`make-test-media.sh --screens`) and builds the UI tests once
(`xcodebuild build-for-testing`). Then every simulator walks at the same
time, each against a disposable server of its own, started as the other
checks do, so one walk's playlists, sessions and Live TV connection never
meet another's. Each server gets a household and libraries: the administrator
"Admin" and the managed profile Kids, each with a random PIN, and Movies,
Shows, Clips and Music, and once they are scanned the collection "Test
collection" of two movies. It starts `live-source.sh` on 127.0.0.1 and
adds it as the Live TV source "Test channels" with one connection.
`live-source.sh <dir> <port>` is a made-up IPTV provider: an M3U lineup
of four channels in three groups (H.264, MPEG-2, 720p50 and a radio
station, each with a logo), an XMLTV guide of half-hour programmes, and
live HLS from FFmpeg's test pictures and tones, served until it is
stopped. Playlists the walk makes are deleted, by
title, before and after it.
It boots each simulator afresh, removes the app from it and empties the
simulator's Keychain, which outlives the app, so the first test starts at
the servers; the tests after it keep that sign-in rather than sign in
again, and only the sign-in walk, and those that check what signing out
does, sign out. A lock per simulator keeps two runs of the script off the
same one. It runs `PleyUITests` on iPhone 17 and iPad (A16)
(turned to landscape, where the sidebar shows) and `PleyTVUITests` on Apple
TV 4K (4th generation) with `xcodebuild test-without-building`, and stops
the servers and deletes their folders afterwards. `PLEY_IOS_SIMS` (names separated by `;`) and `PLEY_TV_SIM` name
other simulators, `PLEY_SHOTS` a folder for the walk's screenshots, and
`PLEY_SCREENS_LOGS` a folder for the logs of a simulator that failed and
the last minute of each failed test's screen recording, and
`PLEY_UI_TESTS` (names separated by `;`, such as `testMusic;testLiveTv`) the
tests to run, all of them when unset. A
test that fails runs once more, on its own in a new test runner
(`-only-testing`), after its errors are printed, so a test that passes
the second time still shows; the tests that passed aren't run again; one that
fails twice fails the run, which then prints the test's errors and the server's
last 40 log lines, with addresses, tokens and codes left out. The server's
address reaches the tests as a `TEST_RUNNER_` variable; the
administrator's token and the PINs are in a file only its owner reads
(`PLEY_SECRETS_FILE`), never in the environment, and go with the
server's folder (a quick run's, when it is stopped). Nothing the script
prints holds them. Sign out is
checked on the server too: the app's grant leaves the administrator's
signed-in apps (`GET /api/users/{id}/apps`). Run without the script, the
UI tests skip.

For work on one test or one screen, `PLEY_SCREENS_QUICK=1` makes a run
quick to repeat. It keeps each simulator's server, with its household,
media and Live TV source, and the booted simulator with the app signed in,
for the next quick run; it starts a kept server again when the server's
binary changed, and empties the simulator only when its server is new. A
failed test isn't run again. With `PLEY_UI_TESTS` and one simulator, a run
after the first is the app's build and the test itself:

```sh
PLEY_SCREENS_QUICK=1 PLEY_IOS_SIMS="iPhone 17" PLEY_TV_SIM= PLEY_UI_TESTS=testLiveTv \
  apps/apple/scripts/check-screens.sh
```

The kept servers run on 127.0.0.1 until `PLEY_SCREENS_QUICK=stop
apps/apple/scripts/check-screens.sh` stops them. A full run, without
`PLEY_SCREENS_QUICK`, is the check before a pull request.

## Code layout

| Path | What |
|---|---|
| `Pley.xcodeproj` | The project: the apps `Pley` and `PleyTV`, Apple TV's Top Shelf extension `PleyTopShelf` (embedded in `PleyTV`), and the UI tests `PleyUITests` and `PleyTVUITests`, with shared schemes for the apps and tests. Its folders are synchronized, so adding a file doesn't change `project.pbxproj` |
| `Config/` | Build settings (`.xcconfig`), the Info.plist keys, the Mac's entitlements and Apple TV's app group; `project.pbxproj` holds no settings |
| `Pley/` | The iPhone, iPad and Mac app: `Shell/` (the tab bar, the sidebar, Me, and the Mac's menus and About window in `MacCommands`), `Kit/` (the views the screens share: `ArtworkTile`, `TileRow`, `TileGrid`, `ItemActionsMenu`, `ServerImage`, `StateView`, `ToastView`, `FlowLayout`, `StarRating`, the collections' and playlists' cards in `ListCards`, the request screens' posters, stage pills and problem states in `RequestsKit`, and `Metrics` and `Motion` for sizes and Reduce Motion), `Screens/` (requests' in `DiscoverViews`, `DiscoverTitleView`, `RequestsViews` and `RequestsEntries`), and `Assets.xcassets` with the icon ([Icons](#icons)) |
| `tvOS/` | The Apple TV app, laid out the same way: `Shell/` (the tabs and Settings), `Kit/` (the same views, built for the focus engine), `Screens/`, and `Assets.xcassets` with the icon and the Top Shelf's picture ([Icons](#icons)) |
| `TopShelf/` | The Top Shelf extension: `ContentProvider`, which reads the row the app saved ([Top Shelf](#top-shelf)) |
| `UITests/`, `TVUITests/` | The UI tests ([UI tests](#ui-tests)): `ScreensTests.swift` (the tests), `Admin.swift` (the administrator's side), and the steps by accessibility identifier: `Walk.swift`, `AccountWalk.swift`, `BrowseWalk.swift`, `ItemWalk.swift`, `ListsWalk.swift`, `LiveWalk.swift`, `MusicWalk.swift` and `PlayerWalk.swift` on both; `DownloadsWalk.swift` and `LargeTextWalk.swift` (the largest text size) on iPhone and iPad; `RemoteWalk.swift` (Play/Pause and a row's focus) and `ShelfWalk.swift` (the Top Shelf's links) on Apple TV |
| `scripts/` | The checks against a real server ([A server to test against](#a-server-to-test-against)): `check-server.sh`, `check-playback.sh` and `check-screens.sh`; the test media (`make-test-media.sh`, `make-pgs.py`) and the made-up Live TV provider (`live-source.sh`) |
| `PleyKit/` | One Swift package with everything else, a module per target |

PleyKit's modules, each depending only on those before it:

| Module | What |
|---|---|
| `PleyShelf` | Apple TV's Top Shelf, for the app and its extension, on Foundation and CryptoKit only: the row (`ShelfSnapshot`), its files in the app group's container (`ShelfStore`) and the signed links (`ShelfLink`) |
| `PleyModel` | The server's answers as `Codable` values: unknown fields are skipped, missing ones read as defaults, and unknown kinds and states still read; the caps report the apps send; the words shown for codecs, tracks, versions and times (`Words`, the Android app's); requests' answers, events and stages (`Requests`) and their words (`RequestWords` in `Words+Requests`, the Android app's) |
| `PleyCommon` | Server addresses and the server list (the Android app's rules), private addresses, host lookups, the User-Agent, Bonjour (browsing, TXT records, resolving), logging |
| `PleyNetwork` | The HTTP client: one ephemeral URLSession, the plain http rule, redirects, size limits, tokens on the server's own origin with a refresh on `invalid_token`, the API's errors, the sign-in endpoints' client, the server check behind "Add a server", servers found on the network and confirmed (`LocalServers`), downloads to a file resumed with `Range`, the typed API calls (browsing in `PleyAPI+Browse`, with ETags; collections and playlists in `PleyAPI+Lists`; search, history and track defaults in `PleyAPI+Account`; Live TV in `PleyAPI+Live`; downloads in `PleyAPI+Downloads`; subtitle search in `PleyAPI+Subtitles`; requests and Discover in `PleyAPI+Requests`), and the server's events (`EventStream`, `EventParser`), and the network's changes that ask for new proofs (`NetworkChanges`) |
| `PleyAuth` | Signing in: the server's metadata, PKCE, the sign-in sheet, sign-in with a code, the tokens (refreshes, sign-out, the Keychain), the server's proofs before a token goes (`ServerProof`), the scope guard and profile switching |
| `PleyData` | The server list, the open server and its sign-in (`SessionManager`), Home's rows, Live TV's lists, guide and favourites (`LiveTvRepository`), requests' summary, its polling and the news told once (`RequestsRepository`), the server's events gathered into changes and scans (`ServerEvents`), the app's settings (`AppSettings`, under `pley.settings.` in UserDefaults: quality, Up Next, subtitle style, grid size, private sign-in, Always start as, the household last seen, the profile used last, recent searches, the downloads' quality and mobile data), the screens saved for offline (`SnapshotStore`), what the screens and the player take from downloads (`DownloadsService`, `LocalMedia`), and pictures (`ImageLoader`) |
| `PleyCaps` | What this device plays: the probe (VideoToolbox, AVFoundation, the audio route, the screen; a file per platform), the table of Apple's devices, the caps report built from them, and `DeviceCaps`, which reads it again when the screen or audio route changes |
| `PleySubtitles` | Subtitles the app draws: the WebVTT reader, text cues by time, the subtitle sync and its offsets, the server's tracks read with `X-Pley-Complete`, and a port of `packages/picture-subtitles` (the Matroska reader, the PGS, VobSub and DVB decoders, and the pictures by time) |
| `PleyPlayer` | Playing: the play request's rules, sessions on the server and their reports (in order, on the app's queue), the reports kept for later, the recovery rules, the Quality menu and Automatic (`Qualities`, `AutoQuality`, `StruggleMeter`, `NetworkStart`, the remembered choices and ceilings, `StreamWarmer`), skips, Up Next and the queues of collections and playlists, and the audio tracks (`Markers`, `CreditsGate`, `NextUp`, `PlayQueue`, `TrackChoices`), the subtitles' choice and schedule, the playback controller, the AVPlayer engine behind a protocol the tests replace, the Mac player's keys, Live TV (`LivePlayback`, with the `AVLiveEngine` behind a protocol), music (`MusicPlayback`, `MusicQueue`, the `AVMusicOutput` behind a protocol), the sleep timer (`SleepTimer`), the system's Now Playing and remote commands (`NowPlayingCenter`, `SystemNowPlaying`) and the audio session (`AudioSessionController`) |
| `PleyDownloads` | Downloads (iPhone, iPad and Mac): `Downloads` (the screens' `DownloadsService` and the player's `LocalMedia`), `DownloadStore` (the folders and `index.json`), `DownloadPlanner` (the rules), `DownloadRunner` (one download at a time, conversions, pieces), `PieceTransfer` (the background `URLSession`) and `NetworkGate` (Wi-Fi or mobile data, from `NWPathMonitor`) |
| `PleyPlayerViews` | The system's player around the controller on each platform (`PlayerSurface`: AVPlayerViewController on iPhone, iPad and Apple TV, AVPlayerView on the Mac), its menus (Quality, Audio, Subtitles and their sync, Find Subtitles…, Speed, Sleep Timer, Chapters, Details), Picture in Picture's keeper (`PictureInPictureKeeper`), Live TV's video (`LiveVideoSurface`, an `AVPlayerLayer`), the subtitles drawn over the video in the system's caption style with the app's overrides (`SubtitleLayer`, `CaptionStyle`), Apple TV's contextual actions and chapter markers, and the AirPlay button (`RoutePicker`) |
| `PleyScreens` | What the screens show and do, shared by both apps without any views: what fills the window (`AppModel.root`), the signed-in sections and their navigation, sheet and toast, and what the Mac's View menu and Reload act on (`ShellModel`), what Play (⌘↩) plays (`PlayTarget`), the servers, signing in (with the QR code, from Core Image), profiles, Home and its tiles (`Tile`, the Android app's rules), a title's actions (`ItemActions`, `ItemActionsModel`), the libraries and their order (`LibrariesModel`), a library's page (`LibraryModel`, `LibraryParams` with the web's query, `LibraryGrid`, `AlphaIndex`), Me, a title's page (`ItemPage`, the Android app's `itemPage`, and `ItemModel`), a person's page (`PersonModel`), music's pages (`ArtistModel`, `AlbumModel`), Now Playing's words (`NowPlayingModel`), Live TV (`LiveTvModel`, `LivePlayerModel`, `ChannelPaging`, and `LiveTvRules` and `GuideLayout`, the Android app's rules), Find Subtitles… (`SubtitleSearchModel`), requests (`DiscoverModel`, `DiscoverResultsModel`, `DiscoverTitleModel`, `RequestsListModel`, the cards' words in `RequestCards`, the actions and Approve… in `RequestActions`, and `RequestRules`, the Android app's), collections (`CollectionsModel`, `CollectionModel`), playlists (`PlaylistsModel`, `PlaylistModel`, `AddToPlaylistModel`, `NewPlaylistModel`), smart filters in words (`SmartFilter`, the Android app's), search (`SearchModel`), Me's track defaults (`MeModel`), Downloads (`DownloadsModel`), History (`HistoryModel`), Settings, Diagnostics and About (`SettingsModel`, `DiagnosticsModel`, `AboutModel`), Apple TV's start (`StartGate`) and its Top Shelf row (`TopShelfWriter`), as `ObservableObject`s; `AppModel` also owns the one playback at a time, music, the system's Now Playing, the device's caps, the session reports and the reports kept for later, one set per server and profile |
| `PleyTesting` | What the tests share: the fixtures, a fixture server (real HTTP on 127.0.0.1, with a household, the sign-in endpoints, tokens and the server's proofs, playback sessions with keyed media, libraries with watch marks, My List and orders that change, collections and playlists, Live TV's lineup and guide, music to play, downloads and conversions, subtitle search, requests and Discover from the Android app's fixtures (`FixtureServer+Requests`), an event stream the tests push to, and a journal of the requests it got), small MP4 files made at test time with AVAssetWriter, and the administrator's side of the end-to-end checks |
| `pley-apple-check` | A command-line tool, on the Mac only, that signs in to a real server ([A server to test against](#a-server-to-test-against)), or prints this Mac's caps report (`--print-caps`) |
| `pley-apple-play` | A command-line tool, on the Mac only, that plays a real server's test media through the player ([A server to test against](#a-server-to-test-against)) |

The rules: modules depend only downwards; the two apps never import each
other's code, and their views hold no logic of their own beyond layout (it
lives in `PleyScreens`, where it is tested); no third-party Swift package.

## A server to test against

Never test against a Pley server people use: signing in creates grants.
The package tests use the fixture server. For a real one, a script builds
the server, starts a disposable one and signs in to it as the apps do:

```sh
PLEY_CHECK_PASSWORD=$(openssl rand -hex 16) apps/apple/scripts/check-server.sh
```

It needs Rust and the web app built in `apps/web/dist`. The server gets a
temporary data folder, a free port on 127.0.0.1, an administrator
`checkadmin` with that password, and no Bonjour or tray.
`pley-apple-check` then signs in with a code as Apple TV, and again as
iPhone:

1. It checks the server and its metadata, and asks for a code.
2. It allows the code as the administrator, through the consent page's
   endpoints (`/api/oauth/device/verify`, then `/api/oauth/requests/{id}`).
3. It collects the tokens, with the server's id and the sign-in's proof
   key, has the server prove itself with that key, and checks that a proof
   under another key is refused ("another device answers there") with no
   token sent. It then fetches `/api/auth/me` (with the scopes asked for:
   the app's four, and `requests` and `approvals` where the server lists
   them; no administrator rights), the household and Home.
4. It refreshes once, checks the proof key stayed the sign-in's first, and
   checks that `/api/me/apps` names the grant
   ("Pley for Apple TV", device `tvOS 18.2; Apple TV`).
5. It signs out, and checks that the old token gets `401`.

The server is stopped and its folder deleted afterwards.

Playback is checked the same way, with the server's transcoder:

```sh
PLEY_CHECK_PASSWORD=$(openssl rand -hex 16) apps/apple/scripts/check-playback.sh
```

It builds the server and its transcoder, makes the test media with FFmpeg
(`scripts/make-test-media.sh`; set `PLEY_CHECK_MEDIA` to a folder to make
it once and keep it), and starts a disposable server as above, its
transcoder working in the same temporary folder. `pley-apple-play` makes
the Movies, Shows and Music libraries, signs in with a code as the app
does (the server proving itself with the sign-in's key before any token
goes), and plays each file through `PlaybackController` and AVPlayer with this Mac's
caps report, decoding without a window. It checks, through the server's
activity, the session's status and the item:

- an H.264 MP4 plays as it is: pictures, the position after 22 seconds,
  the dashboard showing it playing, still there after 35 seconds paused,
  and after a seek and closing, the place saved within 1.5 seconds and the
  session gone;
- an MKV with H.264 and AC-3 is repackaged, in fMP4 segments
  (`#EXT-X-MAP` and `.m4s` in its playlist), its master playlist saying
  `CLOSED-CAPTIONS=NONE`;
- HEVC HDR10 is repackaged with `VIDEO-RANGE=PQ` on a screen that shows
  HDR, and converted to SDR, saying so, on one that doesn't;
- MPEG-2 is converted, and DTS repackaged with its audio converted;
- MKVs with Opus and with FLAC are repackaged with their audio made AAC,
  and an MP4 with Opus plays as it is, with its sound;
- the show's first episode, its markers asked for and made from its
  chapters: Skip Intro lands past the intro, entering the credits brings
  up Up Next for the second episode, and Play Next (past 90 %) marks the
  first watched, ends its session and plays the second;
- the MP4 with two audio tracks switches to its AC-3 track in the same
  session, and the server keeps the choice (`selected_audio`,
  `audio_picked`);
- subtitles drawn by the app: the H.264 MP4's SRT beside it shows its
  second line at 8.5 seconds, from the server's WebVTT, while the file
  plays as it is; the MKV's PGS shows its 480×96 picture at 12.5 seconds,
  from the server's `.mks`, while the session stays repackaged with
  nothing burned in;
- music, through `MusicPlayback` and AVPlayer: the album's two AAC
  tracks play as they are, each in a session of its own, the first's
  session gone once it runs out and the second starts; its MP2 track is
  converted; and stopping ends the session;
- Live TV, with `live-source.sh`'s channels added as a source with one
  connection: through `LivePlayback` and AVPlayer, Test One tunes and
  plays; Test Two takes its place in a new session, Test One's playlist
  then answering 410 or 404 and one viewer left; and leaving ends the
  session within 2 seconds;
- downloads, through `Downloads` with a foreground session in 1 MiB
  pieces: the H.264 MP4's original comes byte for byte, and the MKV with
  AC-3 comes as an MP4 the server made, which AVFoundation plays; then
  the MP4 plays from the Mac with the server out of reach (Details says
  Downloaded, and the server has no session for it), its stop is kept,
  and once the server is reached it gets there with its age, the place
  saved within 1.5 seconds;
- for every recorded Apple device class's caps report
  (`apps/server/tests/fixtures/caps`), the server's decision for eight of
  the files (method, converted audio and reasons) is the one
  `playback_apple.rs` expects.

It prints a line per check and stops at the first failure; nothing it
prints, nor the server's lines it shows on a failure, holds a token or a
keyed address.

`scripts/make-test-media.sh <dir>` makes, from FFmpeg's test pictures and
tones: an H.264 MP4 with an SRT beside it, an MP4 with two audio tracks, an
MKV with H.264, AC-3, an SRT and a PGS track (written by
`scripts/make-pgs.py`, since FFmpeg can't encode PGS), HEVC HDR10 with
E-AC-3, MPEG-2 with MP2, H.264 with DTS, AV1 10-bit, MKVs with Opus and
with FLAC, an MP4 with Opus, a show's two episodes with an intro and
credits chapters, and an album of two AAC tracks and an MP2 one. It needs
FFmpeg with libx264, libx265, libsvtav1 and libopus. With `--screens` it makes the UI tests' media instead: the H.264
MP4, the show with a third episode, forty 2-second clips (rows and grids
longer than a screen) and an album of two tracks.

To try the apps
by hand, start a server the same way and keep it running:

```sh
data=$(mktemp -d)
pley --data-dir "$data" reset-password admin <password> --admin
pley --data-dir "$data" --bind 127.0.0.1:7712 --no-discovery --no-tray
```

The simulators share the Mac's network, so the apps open
`http://127.0.0.1:7712`. To have it found by Bonjour, leave out
`--no-discovery`, bind a LAN address and turn Bonjour on in Settings →
Network (or put `bonjour = true` in the data folder's `pley.toml`).
Delete the folder afterwards.

## Known limits

- No seek-bar previews.
- Subtitles aren't shown over AirPlay, nor in Picture in Picture: the app
  draws them over its own view of the video, which neither shows.
- Downloads are on iPhone, iPad and Mac only, as Android's are on phones
  and tablets only: an Apple TV app keeps nothing large for long (the
  system may clear what it stores to make room). Downloads keep text
  subtitles only, as on Android: picture subtitles (PGS, VobSub, DVB)
  don't come with a download. A conversion the server makes for a
  download is followed only while Pley is open, since following it needs
  the app running: one started then waits until Pley is opened again. The
  file itself downloads in the background.
- On iPhone and iPad the system's player keeps its own 10-second back and
  forward buttons beside Play, which the other apps' players no longer
  have: AVKit hides them only by turning off scrubbing too
  (`requiresLinearPlayback`). Apple TV shows none; its clickpad's Left and
  Right go back and forward 10 seconds, as Left and Right do in the other
  TV apps.
- Live TV has no Picture in Picture and doesn't switch the display's
  frame rate or range, as on Android: its player is Pley's own view of the
  video, with no pause or seeking, not the system's player, which brings
  both.
- Events aren't read while the app is in the background; back in front,
  what shows is read again.
- The Mac app has no UI tests: driving it needs Accessibility and Screen
  Recording permissions that only a person can grant. Its views are
  checked by building it, and its models and menus' state by the
  packages' tests.
- Collections are made and changed in Pley on the web; the apps play
  them.
- People's pages need a title's cast and crew, which come from a metadata
  source (TMDB); without one, a title lists no one.
- Only Apple TV asks "Who's watching?" by itself at start; iPhone, iPad
  and Mac open on the profile used last, which stays chosen until someone
  switches. Always start as a profile with a PIN still asks for the PIN,
  through "Who's watching?", when another profile was in use.
- Plain http reaches IP addresses, names without dots and `.local` names
  only; an http name with dots (`nas.home.arpa`) is refused by the system,
  even when it points to the home network. Such a server is reached by its
  IP address or over https.
- The Top Shelf shows Continue Watching as the app read it last: what
  changes while Pley isn't open shows once it is. A build signed by a
  team without App Groups has no row, only the app's own picture.
- No audio delay, which the Android app has: AVPlayer has no way to play
  a track's sound earlier or later than its picture, and the apps don't
  decode the sound themselves.
- Apple TV has no requests: Discover, asking for a title, approving and
  their news are on iPhone, iPad and Mac, as in Pley on the web and the
  other apps.
- No Handoff: a video started on one Apple device isn't offered on
  another. Continue Watching, on every device, resumes it there.
