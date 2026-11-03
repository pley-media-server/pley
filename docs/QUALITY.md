# Playback quality

The Quality menu every Pley player offers, and how Automatic chooses what to
play. This is the spec for the web app, the Android app (phones and TVs), the
Samsung and LG apps, Pley for Kodi and Pley for Roku; each app's own doc
covers only where it differs.

Contents: [The menu](#the-menu) · [Original](#original) ·
[Automatic](#automatic) ·
[Players that go black between streams](#players-that-go-black-between-streams) ·
[Several sizes](#several-sizes-adaptive-bitrate) · [The server](#the-server)

## The menu

The same on every app, in this order:

| id | Label | Video at most |
|---|---|---|
| `original` | Original | The file as it is |
| `auto` | Automatic | Adjusts to conditions |
| `30m` | 30 Mbps 4K | 30 Mbps, 2160 lines |
| `15m` | 15 Mbps 4K | 15 Mbps, 2160 lines |
| `20m` | 20 Mbps 1080p | 20 Mbps, 1080 lines |
| `12m` | 12 Mbps 1080p | 12 Mbps, 1080 lines |
| `8m` | 8 Mbps 1080p | 8 Mbps, 1080 lines |
| `4m` | 4 Mbps 720p | 4 Mbps, 720 lines |
| `2m` | 2 Mbps 480p | 2 Mbps, 480 lines |

When the player knows the file:

- Original and Automatic are always offered. Another quality is offered only
  if it makes the file smaller: fewer lines, or at most 80 % of its bitrate.
- A quality taller than the file takes the file's lines in its name: a 720p
  file at 12 Mbps is offered "8 Mbps 720p", not "8 Mbps 1080p".
- Qualities that come to the same bitrate and lines show once, sorted by
  lines then bitrate (a 1920×800 film doesn't list "15 Mbps 1080p" above
  "20 Mbps 1080p"). A 4K file keeps the table's order.
- Automatic steps through the qualities offered for the file.

With no file in sight (the default quality in Settings, Kodi's and Roku's
settings), qualities keep the table's names. Automatic is the default until
a quality is picked; then the picked one (Original included) is kept, and a
file that doesn't offer it plays the Original.

What the menu shows:

- While Automatic plays, its label says what plays: "Automatic · Original"
  or "Automatic · 8 Mbps 1080p".
- Original's second line is the file's size and bitrate ("1080p · 8.2
  Mbps"); Automatic's is what plays now ("Now 8 Mbps 1080p · Lowered for
  your connection"). Other qualities have none.
- The menu never says how a choice plays: an Original the server converts
  still reads "Original", and burned-in subtitles carry no mark. The
  player's details say that: how it plays, the server's reasons, what goes
  in and out, and why Automatic went down ("Lowered for your connection" or
  "Lowered for this device").

## Original

The file as it is (`mode: "auto"`): direct play, or a remux when the
container doesn't play on the device. Only what the device can't play is
converted (a codec it doesn't decode, a decoder that fails or can't keep
up), at the server's own quality for the file (`auto_quality`). Original
never lowers the quality for the network: a slow network waits.

## Automatic

Original, unless it doesn't play well; then the highest quality that does.

### Starting

- **Known network**: the player asks the server what it knows of this
  network ([memory](#memory)). If it is slower than the file, playback
  starts at the highest quality it carries, never below 2 Mbps 480p, with
  "Lowered for your connection". Otherwise it starts as Original.
- **Unknown network away from home**: a speed test first (at most 2 s of
  data, during the usual loading). Playback starts at 70 % of the measured
  speed, or as Original if the whole test arrived in time.
- The server's own network is never tested.

### Watching

While the video plays (not paused, not seeking), each of these is a *play
issue*:

| Signal | Play issue when |
|---|---|
| Rebuffering | A wait for data of 2 s or more, or two shorter waits within 60 s |
| Delivery (seconds of video arriving per second, over the last 20 s) | Under 1.1 for 15 s, with less than 10 s buffered and falling |
| Decoding | Dropped frames or a failing decoder, as each app counts them |

**At the Original played as the file is** (direct play or remux), only a
stopped picture counts: a wait for data of 2 s or more. Short waits and slow
delivery don't: a player buffers only seconds of a big file (a browser about
10 s or 50 MB), so they say little about the network, and a transcode would
play a worse picture for the rest of the title. An Original the server
converts counts every signal, as any quality does.

That long wait counts only when the network is to blame:

- **Measured over the last 2 minutes**: it counts when the network carries
  less than 1.5 times the file's bitrate. Above that, the server is the
  cause (a slow disk, a remux catching up), and a transcode would only wait
  longer. The measurement is taken again during and after the wait (not on
  Samsung and LG, whose figures come only while the video plays).
- **Nothing measured**: the second such wait within 5 minutes counts.
- **Web app**: a wait that began with 1 s or more loaded ahead is the
  browser's decoder, and never counts.
- **Android app**: the warning that the server converts slower than the
  video plays doesn't count at the Original either.

Roku measures the network only from HLS segments, and Kodi only from whole
answers through the add-on's proxy, so their direct plays have nothing
measured.

### Stepping down

On a play issue, the new ceiling is:

> bitrate playing now (the file's for Original, the quality's for a
> transcode) × delivery seen × 0.75

capped at the server's own quality for the file (`auto_quality`, at the
file's lines or fewer). Automatic plays the highest quality at or below it,
never below 2 Mbps 480p. A decoding issue at Original goes straight to the
server's own quality.

- The session restarts where it was (`mode: "transcode"` with the quality's
  `max_bitrate` and `max_height`), and the player says once: "Quality
  lowered to 8 Mbps 1080p".
- Further play issues step down again, at least one quality, but not within
  20 s of the last change.
- The web app and the Android app prepare the new session while the old one
  plays (`keep_previous`), request its first segment, and switch on the old
  one's last picture with no spinner. They do the same for a quality, audio
  or picture-subtitle change picked in their menus.
- The Samsung and LG apps do the same, requesting the segment with `HEAD`
  ([TV_PLATFORMS.md](TV_PLATFORMS.md#quality)), but their players can't
  hold the last picture: the screen is black while the new stream opens,
  and their loading ring waits 2.5 s.

### Stepping up

- After 3 minutes without a play issue, and not within 5 minutes of a step
  down, Automatic steps up one quality if the network measured over that
  time carries 1.6 times the next quality up (or the file, for Original).
  At most one step every 3 minutes, silently.
- A title that started lower because of the [memory](#memory) needs only
  20 s without a play issue (web app and Android app), then goes in one step
  to the highest quality the network carries 1.6 times, Original included.
  The memory may come from a slower moment, or from a step down the server
  caused. After a play issue or a step, the usual rules apply.
- Players measure the network from what they download: an HLS segment's
  size over the time since its first byte (the wait before it is the
  transcoder's), or a direct play's bytes read while filling its buffer. A
  player that can't measure stays where it went down to.

### Settling

A step up that a step down undoes within 10 minutes was one too far:

- Automatic doesn't step up to that quality again during the title.
- The calm it waits for before any other step up doubles each time (3, 6,
  then 12 minutes).
- The network memory keeps the settled step, so the next title starts there.

### Memory

The server remembers what a network carries, per profile, device and
network, as a keyed hash of the network address (never the address):

| What | Kept for |
|---|---|
| The bitrate of the last step | 24 hours |
| A speed test result | 7 days |

- The next title in Automatic (Up Next included), or the same device back on
  that network, starts at most at that bitrate.
- A step down, or up to a quality, updates it. A step back up to Original,
  or picking Automatic in the menu, clears it, and no speed test runs on
  that network for 7 days.
- Picking any quality in the menu, Automatic included, starts afresh from
  that choice.
- If the server can't be reached, the device keeps its own memory for 30
  minutes, per device and server: the bitrate at the last step down.

## Players that go black between streams

Samsung's AVPlay and Kodi's player close one stream before opening the
next, and Roku's Video node replaces its content the same way. LG's player
is treated the same. (The web app and the Android
app switch on the last picture, so this section doesn't apply to them.)

On these players Automatic never switches streams while one plays well. It
changes the stream as it is sent instead:

- **A conversion changes its bitrate, not the stream.** The player calls
  `PUT /api/playback/{id}/bitrate`, and the transcoder makes the segments
  not yet fetched at the new bitrate, same size, same playlist, so playback
  goes on without a pause. Starting, stepping and settling follow the rules
  above.
- **A stream keeps its size** (4K stays 4K), with a bitrate floor: 8 Mbps
  for 4K, 3 Mbps for 1080p, 1.5 Mbps for 720p, 1 Mbps below. Under the
  floor, the network waits.
- **Steps**: down to the ceiling in whole Mbps ("Automatic · 4 Mbps
  1080p"). Up, Samsung and LG go to the next of the file's qualities'
  bitrates, Kodi and Roku through the menu's bitrates (2, 4, 8, 12, 15, 20,
  30 Mbps), then all to the stream's own bitrate, never above it.
- **A stream that can't go higher** without a new stream (at its own
  bitrate, or one whose bitrate can't change) stays while it plays well. On
  Kodi and Roku, when the calm minutes find the network carrying 1.6 times
  the next quality up, the network memory goes up a quality instead, so the
  next title starts there; at the original it is cleared.
- **Playing the original**, it moves to a conversion only once the picture
  has stopped anyway ([watching](#watching)) or the decoder fails. Dropped
  frames move it only in a title's first minute (the device can't keep up
  with the file); later they lower where the next title starts. On Kodi and
  Roku, a conversion whose bitrate can't change (an older server or
  transcoder) is treated the same way.
- **One size per stream on Samsung and LG** (they don't send
  `caps.hls.variants`): a size change inside a stream may blank their
  screens too. Roku's Video node switches sizes without a gap, so Roku keeps
  them.

## Several sizes (adaptive bitrate)

A player that switches between sizes by itself sends `caps.hls.variants`
(hls.js, the browsers' own HLS, Media3, Roku's Video node), and its
transcodes come in several sizes. Kodi and the Samsung and LG apps don't.

- `master.m3u8` lists the transcode's own size first. A quality with more
  bits than its size usually gets (20 Mbps 1080p, 30 Mbps 4K) comes again at
  that size with the usual bits (8 Mbps for 1080p, 20 for 4K).
- Below come 1080p, 720p, 540p and 360p, at 8, 4, 2 and 0.7 Mbps, never more
  than the size above. One with more than two thirds of the bits of the size
  above is left out.
- A picked quality is the top size; nothing goes higher.
- Every size has the same segments, each starting with a keyframe at the
  same time, so the player can switch at any segment without a pause, a new
  session or a sound glitch.
- The transcoder makes only the size the player requests; the others stop,
  keeping what they made. A size it can't make in real time arrives slowly,
  and the player moves down as for a slow network.
- The players' ABR moves up only with room to spare: hls.js counts half the
  measured bandwidth for a move up (`abrBandWidthUpFactor` 0.5), and Media3
  waits for 20 s of buffer (`minDurationForQualityIncreaseMs`).

While a session has several sizes (`PlaybackInfo.adaptive`), Automatic
leaves the network to the player's ABR and watches only for what sizes can't
fix: dropped frames, and a stall long enough to ask the server. Both still
restart the session lower; other play issues only restart the calm. The web
app and the Android app step back up from such a session as
[above](#stepping-up), to the next quality or the Original, so a step down
from the Original doesn't last the title.

The web app's Automatic plays no size bigger than the player on screen; a
picked quality plays its own size whenever the connection allows. The menu
shows the size playing ("Automatic · 720p"), and the dashboard which of the
sizes ("size 2 of 4").

## The server

| Player asks for | Request |
|---|---|
| Original | `mode: "auto"` |
| A quality | `mode: "transcode"` with `max_bitrate` and `max_height` |

- A quality is converted at its bitrate, less 192 kbps for the sound.
- It never goes above half again the file's own bitrate, nor above the
  highest quality of its lines (what an app of the Jellyfin-compatible API
  asking for far more gets). A file with fewer bits still gets the lowest
  quality of its lines.
- The response's `auto_quality` is what the server would choose for the
  file, and `adaptive` says whether the stream comes in several sizes.
- `GET /api/playback/network` reads the network memory,
  `GET /api/playback/speedtest` is the speed test, and
  `PUT /api/playback/network` reports both
  ([API.md](API.md#network-memory-and-speed-test)).
