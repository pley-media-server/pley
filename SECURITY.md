# Security

How to report a vulnerability, how to run Pley safely, and what Pley does to
protect your server.

## Reporting a vulnerability

- Report it privately on GitHub: **Security → Report a vulnerability** on
  this repository. Please don't open a public issue.
- Include the version (`pley --version`), how Pley is deployed, and the steps
  to reproduce.
- Only the latest release gets fixes.

## Running Pley safely

- **Use HTTPS beyond your home network.** Pley speaks plain HTTP. When it can
  be reached from outside, put it behind a reverse proxy with TLS that sets
  `X-Forwarded-Proto: https`; Pley then marks its cookies `Secure`. The
  desktop app warns before connecting to an unencrypted address on the
  internet, and the Android app refuses one.
- **Behind a reverse proxy, pass the original `Host`** and set
  `X-Forwarded-Proto`, overwriting what clients send
  ([deploy/README.md](deploy/README.md#apps-and-reverse-proxies)).
- **Set Pley up before exposing it.** A new server is set up with the
  one-time code Pley prints in its log (the tray icon's "Open Pley" fills it
  in).
- **Keep what doesn't need to be public off the internet:** a transcoder on
  another computer, and a compatibility API's own port (such as 32400 or
  8096) if you gave it one: on Pley's port it is behind your reverse proxy's
  HTTPS like the rest.
- **Give each tool a token of its own, and no more than it needs.** The
  compatibility APIs take only tokens made for other apps. Make one per app
  or tool in **Account → Tokens for other apps**, allow only what it needs,
  limit it to the addresses the tool runs at, and revoke what isn't used.
  Turn an API off (`plex_bind = "off"`; the Jellyfin-compatible one is off
  by default) if nothing needs it.
- **Mount media read-only**, run Pley as an unprivileged user, and don't let
  untrusted people write to library folders. The Docker setup does the first
  two and more: a read-only image, no capabilities, the port on 127.0.0.1
  only ([deploy/docker/README.md](deploy/docker/README.md#security)).
- **Turn on two-step sign-in** for administrators of a server reachable from
  the internet (**Account → Two-step sign-in**), and keep the recovery codes
  somewhere safe. To have everyone use it, require it
  (**Settings → Server → Sign-in**).
- **Leave apps by other publishers off** (**Settings → Server → Apps**)
  unless you need one.

## What Pley does

### Setup

- Creating the administrator account takes the setup code from the log,
  wherever the request seems to come from: Docker, reverse proxies and DNS
  rebinding can make the internet look local.
- Guesses are limited to 5 every 15 minutes from each address (behind a
  proxy, each client's), and 1,000 from all of them.
- A restart makes a new code.

### Passwords and PINs

- Passwords are hashed with Argon2id. They need at least 8 characters, and
  can't be the username or a common password.
- Sign-ins are limited to 5 failures every 15 minutes per account from each
  address, and 50 from all addresses, so someone guessing elsewhere doesn't
  lock the owner out.
- PINs, and the current password when changing it, are limited to 5 failures
  every 15 minutes per account.
- A password or PIN check Pley couldn't run (too busy, or out of memory)
  answers that it is busy, and counts toward none of these limits nor a
  PIN's lock.
- An unknown username costs a sign-in as much as a real one: it is checked
  against a real hash, and while that hash can't be made the sign-in is told
  Pley is busy, as a real user's would be.
- Password checks queue, and failures are forgotten when their window ends,
  so a flood of sign-ins can't exhaust memory. When the table of counts
  fills, counts that block nobody give way: lockouts stay and everyone else
  signs in.
- A PIN stops working after 100 wrong tries in a row, until it is set again.
- A new password signs the account out on every other device.
- `pley reset-password <username>` asks for the new password, or reads it
  from standard input. It also accepts the password as an argument, but
  leave it out: the shell's history and the list of processes would keep it.

### Two-step sign-in

- Optional, unless an administrator requires it. After the password, it
  takes a code from an authenticator app (TOTP) or one of ten recovery
  codes.
- Turning it on takes the password and a first code; turning it off, the
  password and a code. Moving it to another authenticator app takes the
  password and a code of the current one, then the new app's first code.
- A code works once. Wrong codes are limited to 5 every 15 minutes per
  account, and after 100 in a row only a recovery code signs in.
- Authenticator secrets are encrypted with a key kept in the data folder
  (`secret.key`), so a copy of the database or its backups can't make codes.
  Recovery codes are stored hashed.
- Switching to a profile with a password takes its code too. Apps sign in on
  the same page, so they take the code as well.
- **Required of everyone** (`require_two_step`): only an administrator who
  uses it can turn it on, so nobody locks themselves out. Everyone with a
  password and without it is signed out everywhere, browsers, apps and
  tools; no session, app token or token for other apps of an account
  without it works while it is required, checked on every request and
  refresh. Each sets it up at their
  next sign-in, after the password, before any session exists; a PIN never
  sets it up. It can't be turned off then, only moved to another app.
- Until someone sets it up, their password alone lets whoever has it set it
  up with their own authenticator app. A secret being set up goes only to
  the sign-in it was shown to: another one with the password gets a new
  secret, which replaces it, so the owner sees their QR code stop working
  rather than share it unknowingly. Once it is required, check
  **Settings → Users** for people who haven't set it up yet. An
  administrator who resets someone's two-step sign-in signs them out too.
- While the server can't use `secret.key` (the log says why at start),
  authenticator apps can't be set up or their codes checked. Requiring it
  holds all the same: accounts that haven't set it up can't sign in at
  all, and accounts that have sign in with a recovery code. An
  administrator who signs in so can stop requiring it (**Settings →
  Server → Sign-in**).
- An administrator with no recovery code left then, or whose two-step
  sign-in `pley reset-password` turned off, can't sign in until
  `secret.key` is fixed on the server and Pley restarted: give the file
  back to Pley's account (after a change of PUID and PGID, say), restore
  it from a backup of the data folder, or delete it for Pley to make a new
  one (authenticator apps set up before then stop working; recovery codes
  still sign in; apps say another device answers at the server's address
  until they sign in again, as its proofs change, and the desktop app until
  the server is forgotten and connected again).

### Sessions

- Session tokens are random 256-bit values, stored hashed.
- Browsers keep them only in an `HttpOnly`, `SameSite=Lax` cookie. No answer
  gives a page's script the token.
- A request that changes anything with the cookie must come from Pley's own
  page (`Sec-Fetch-Site`, or `Origin`), since a page on another port of the
  same host is the same site to the browser.
- Other sites' pages can't make the API fetch anything with the cookie
  (links, pictures) where the browser says which page asked
  (`Sec-Fetch-Site`: over HTTPS, and to `localhost`); only the address bar
  and bookmarks can. Over plain HTTP to another address browsers don't say
  it, so a GET there does no more than read and play: the folder browser,
  which opens whatever folder it is given, is a POST, whose `Origin` says.
  A `?token=` in their address changes nothing, since the cookie is what
  would sign them in.
- The devices and apps lists show the address a request came from as the
  rate limits count it, which nobody on the internet can choose.
- Administrators change their own password, PIN and two-step sign-in in
  Account, which asks for the password (and code), not through the users
  list.
- A session that is signed out, or whose account is disabled or changed,
  stops working at once, even one being checked at that moment. Its event
  streams end within 30 seconds, when they check it again.
- Switching from a restricted profile to one that can see more (an
  administrator, a higher age limit, other libraries, fewer labels it
  never sees or more it always sees) takes that profile's PIN, or its
  password (and code).
- A profile's limits (libraries, age limit, label rules) are kept by the
  server in every listing, search, stream and compatibility API, and
  Discover; a hidden title answers as if it didn't exist. Only
  administrators see a title's labels and a profile's rules, so a child
  doesn't read what a label says. The limits decide what Pley shows, not
  what's on the disk: anyone who can read the media folders sees it all.
- A remote casting to a device of another of the household's profiles
  switches that device to its own profile, signed in as it already, by a
  switch's rules from the device's profile: a freer profile (an
  administrator, a higher age limit, other libraries, freer label rules)
  takes its PIN and,
  without one, can't cast there, so a child's device never becomes an
  administrator's without that PIN. An administrator's profile is cast
  only to Pley's apps, whose tokens never carry administrator rights,
  never to a browser's or the desktop app's sign-in, which would stay one
  with those rights, and such a sign-in never switches to an
  administrator with a ticket. When the profile has a PIN, the remote
  gives it first, checked and throttled as a switch's, and the rules are
  checked again as the device switches. Only devices whose app switches
  (Pley's own, allowed to switch profiles) are offered across profiles,
  so an older app never plays someone else's cast as its own profile. The
  device switches with a ticket the cast carries: random, stored hashed,
  good once for 60 seconds for that device, from the profile it is on to
  the caster's. A copy shown by anything else spends it. Such a cast goes
  to the device the remote listed or to none, and reaches that device's
  stream alone, not the profile's other devices. A receiver is its
  device's: another sign-in of the profile that names a TV's receiver id
  is listed beside the TV, never in its place, whether the TV is there or
  comes back later, and never gets what is cast to the TV. A progress
  report of another profile's session is kept for neither profile. Another
  profile's devices are listed by name and profile, never with what they
  play, and only to home members of the same household, from a person's
  own sign-in or Pley's own apps allowed to switch profiles (another app
  could take a TV over as its profile), nor are anyone's devices outside
  it, or a disabled profile's, listed or cast to from it.

### Apps signing in

Apps sign in on Pley's own page with OAuth 2.1.

- Authorization codes need PKCE (S256 only), work once, and expire after 60
  seconds. Return addresses must match exactly (only a loopback port may
  differ). Every answer carries `iss`.
- Tokens are random, stored hashed, and sent only in the `Authorization`
  header. Access tokens last an hour. Refresh tokens rotate on every use, and
  one used twice ends the app's access.
- People are asked every time, Pley's own apps included, on a page that
  shows the app's address first and can't be framed.
- Apps never get administrator rights but two, which only Pley's own apps
  get, on an administrator's account and when the administrator allows
  them: approving requests (`approvals`), and seeing and stopping
  everyone's streams (`activity`, which only Pley for Home Assistant asks
  for). Whatever holds such a token can do as much, so whoever controls a
  Home Assistant an administrator signed in can see what everyone plays and
  stop it. Other apps, and apps on other accounts, see and stop only their
  profile's own streams. Every route apps may use is listed with the scope
  it needs, anything else is refused, and the compatibility APIs refuse
  their tokens.
- Apps, and people who aren't administrators, don't see library folders or
  file paths on either API (only a file's name), the transcoder's error
  messages included.
- Pley's own apps are built in and sign in without internet access.
- Descriptions of other publishers' apps are read only when an administrator
  allows those apps, for someone signed in, from public addresses only,
  without redirects, and up to 5 KiB. Reading one tells the app's host that
  this server is in use.
- A new password, **Sign out everywhere**, `pley reset-password` and deleting
  the account sign out the account's apps along with its other sign-ins.
  Apps it was signing in get nothing: a code is exchanged only while the
  browser session that allowed it is signed in, and unexchanged codes end
  with the account's sign-ins.
- App sign-in works over plain HTTP at home, as passwords do. Beyond the home
  network it needs HTTPS, like the rest of Pley.
- Each sign-in gets a key of its own, derived from `secret.key`, with which
  the server proves it is the server an app signed in to before the app
  sends its token to an address (every app). Another device at that
  address can't, nor can another app's key. The proof covers
  the address it was asked at; the server makes none for a plain-http
  address to a client on the internet, none for an https address but
  through a proxy on the home network, and none for a page other than
  Pley's TV apps, so neither a device elsewhere nor a website's page open
  at home can pass a challenge on for it. One that reaches the server from
  the home network itself, or through a proxy that hides where clients
  come from, could, as could a page in an LG TV's own browser, and a
  network that gives a plain-http name another device's address; an https
  address is checked by its certificate too, and its proxy must pass on
  only the server's own names, or the server proves itself under a name
  someone else may hold now.
  Refresh tokens and PINs wait for a proof made then; an access token may
  use one made within the half hour on the same network. The desktop app,
  which uses the web app's session, asks for a key of its own with that
  session; the server's pages in its window send the session themselves,
  and leave a plain-http server within a second of the computer's way to
  it changing, or of its waking from sleep.

### Apps and tools using the compatibility APIs

Tools built on Plex's API (Kometa, Tautulli, scripts), Jellyfin's apps and
other players use the Plex- and Jellyfin-compatible APIs with tokens for
other apps (`pley_ct_…`), never with a session's
([COMPATIBILITY.md](docs/COMPATIBILITY.md)).

- Tokens are random 256-bit values, stored hashed. Pley shows one once,
  when it is made on the account page, or to a Jellyfin app signing in with
  its account's password, which goes through the checks of Pley's own
  sign-in (attempt limits, disabled accounts), takes a JSON body only, and
  makes no session. An account with two-step sign-in can't sign in this way,
  nor one without it while the server requires it: the API has no second
  step.
- They work on the compatibility APIs only, from those APIs' token headers
  or query parameters, `Authorization: Bearer`, `X-Pley-Token` or
  `?token=`, never from the cookie. Pley's own API refuses them wherever
  they are sent, and the compatibility APIs refuse sessions' and apps'
  tokens.
- A token does what its scopes allow: `play` (browsing, playing, its own
  account's watch state, sessions and playlists) always; `activity`
  (seeing every account's sessions, history and accounts, and where files
  are) and `manage` (refreshes, edits, collections, stopping anyone's
  playback) only for an administrator, checked at every request, so a role
  taken away takes them at once. Each is checked by name where it applies:
  no token is ever an administrator. No scope administers accounts, sign-in
  or the server's settings, and the Jellyfin-compatible API takes every token
  for what `play` allows, and one with `manage` for collections and editing
  metadata: it lists no one but the caller, a user id in a path must be its
  own, and a playlist is its owner's alone. A picture it is sent to fetch
  must be on one of the hosts Pley takes pictures from, as on the
  Plex-compatible API.
- A token may be limited to addresses and networks (or the home network).
  From elsewhere it is refused as if it didn't exist; the address is the
  connection's, or the one a proxy on this computer or its network names
  (`X-Forwarded-For`). A proxy that passes the header on without adding
  the client's address (or forwards bare TCP) lets the client choose it,
  so the limit holds only behind proxies that add it.
- On Pley's port, only requests that name themselves an API's (its headers
  or token) or paths it alone has reach it; `/api/…` never does. Answers
  are never pages (a sandboxing `Content-Security-Policy`, `nosniff`, no
  framing), and vary with the headers that decide for caches.
- Players open streams, subtitles, seek previews and pictures without a
  token. A stream's HLS session is reached by its 128-bit id, and a
  Jellyfin media source by an id keyed with the server's `secret.key` (an
  HMAC): either
  stands for the token that started or listed it, and works only while that
  token does, from where it may be used, for what its user may see. A
  Jellyfin picture answers for an id someone was given, and with a token
  only for what its user may see. Without `activity`, files are shown by
  their names, never where the server keeps them.
- A token ends when it is revoked, after 90 days unused, and with the
  account's other sign-ins: a new password, **Sign out everywhere**,
  `pley reset-password` and deleting the account. It doesn't work while the
  account is disabled, nor while two-step sign-in is required of an account
  that doesn't use it. A revoked token stops working at once, even one being
  checked at that moment, and so do the streams it started.
- An account keeps 100 tokens at most; making another ends the least
  recently used.

### TVs signing in

- Pley's own TV apps show an 8-letter code (about 2.6 × 10¹⁰ possible codes).
  Someone signed in types it on Pley's `/link` page, sees the same consent as
  for other apps, and is told to allow only a code on a screen in front of
  them (RFC 8628 §5.4).
- Only Pley's own apps can start one.
- At most 20 codes are in progress from one address (an IPv6 /64), 50 from
  one home's /56, and 1,000 in all. None is dropped to make room.
- After 10 wrong codes from one person, or 300 from everyone, in 10 minutes,
  no code is looked up.
- A TV that asks too often is slowed down.
- The answer is given once. A TV allowed before the account's sign-ins end,
  or from a session signed out since, gets nothing.
- A signed-out app's number is never given to another, so its keyed media
  addresses stay dead.

### Pages and connections

- A `Content-Security-Policy` allows only Pley's own scripts and styles.
- No other site may frame Pley (`frame-ancestors 'none'`,
  `X-Frame-Options: DENY`). Answers carry `nosniff` and
  `Referrer-Policy: same-origin`.
- Clients get 30 seconds to send a request's headers, and a body may pause
  for 20 seconds at most.
- At most 8,192 connections are open at once, and 256 from one address on
  the internet. A proxy on this computer or its network carries everyone's,
  so its connections aren't counted.
- A session keeps at most 16 event streams, and an account at most 200
  networks that Automatic quality remembers.
- On Unix, Pley raises its open-files limit to the most it may have.

### Discovery

- Apps find servers on the local network by a UDP broadcast and by
  Bonjour (`_pley._tcp`). Both tell only the server's id, name, version and
  web port. The name and version are on its public endpoint too; the id is
  a random identifier that says nothing about the computer or its users.
  Bonjour, once an administrator turns it on, tells them to every device
  on the local network unasked, and its host name (`pley-` and the id's
  start, unless an administrator chooses one) shows part of the id.
- A chosen host name is probed for before it is claimed, and left for
  the generated one when another device has it. Any device on the
  network can still answer for a `.local` name, so the apps confirm a
  server by its id and public endpoint, never by the name.
- The UDP answer goes only to addresses on the local network, at most 100
  a second. The Bonjour announcement goes out only from adapters with
  addresses on the local network, never to the internet nor, unless
  chosen, to 100.64.0.0/10 (Tailscale's, or a carrier's shared
  addresses), and from only those an administrator chooses when some
  are (a VPN, a container
  network or a guest Wi-Fi left out). Each network is told only the
  server's address on it.
- Bonjour's responder is Pley's own and answers for Pley's names only. It
  answers only devices on the networks it announces on, so a forged source
  address elsewhere gets nothing; multicasts an answer once a second at
  most on each network, and sends 100 answers a second at most by
  unicast, so a device on the network can't make it flood another; reads
  what it hears within bounds; and keeps none of it, so no device can
  make it hold more.
- Discovery is on and Bonjour off until Settings → Network changes them.
  `--no-bonjour` (`PLEY_NO_BONJOUR`) keeps Bonjour off, and
  `--no-discovery` (`PLEY_NO_DISCOVERY`) both; the Docker setup has them
  off.

### Files

- A symbolic link in a library is followed only to a file of the same kind
  (a video to a video, a picture to a picture, an NFO file to an NFO
  file). This is checked when the
  library is scanned and again whenever the file is used: played,
  transcoded, downloaded, or its pictures or subtitles served. A symbolic
  link named like a film can't hand out other files.
- A hard link is the file itself, which no check can tell from any other:
  where the system lets an account link a file it can't read (macOS, Linux
  without `fs.protected_hardlinks`), one that can write in a library folder
  could link Pley's database there under a film's name. Hard links don't
  cross volumes, so keep the data folder on one that such accounts can't
  write in.
- What Pley sends of a library's files itself (a file played as it is, a
  picture, a subtitle file) is opened first, checked as the file opened,
  and sent from that file, so a link swapped in after the check is never
  read.
- A file the transcoder reads (a transcode, a download, a frame grab, a
  cover, subtitles, seek previews) is checked just before it is asked to.
  The transcoder opens it by its name, so a link swapped in at that moment
  could still be read, but only as video, audio or subtitles, the only
  formats its FFmpeg opens: never a database or a key.
- The data folder (database, backups, log, settings) is kept to Pley's
  account: mode 0700 on Unix. On Windows, a folder every account could use
  (one on `D:\`, say) is limited to Pley's account, SYSTEM and the
  administrators.
- On Windows, Pley warns at start when every account may change the folder
  its programs are in, since any of them could replace those programs.
- The Windows service takes a data folder only when it is new or the
  administrators' (one an ordinary user made beforehand would stay theirs to
  open up again), and makes a new one closed from the start. It keeps two
  privileges, never impersonation, restricted or not
  ([deploy/windows/README.md](deploy/windows/README.md#security)).

### Untrusted media

- What FFmpeg hands back for one picture or subtitle track is capped at
  64 MB, and 512 MB for an extracted track. A crafted file's subtitle styles
  and MP4 sample tables can't take more memory than a real one's.
- Pictures come only from the metadata services' hosts over HTTPS,
  redirects included, and are read up to 25 MB whatever their headers say.
  Live TV's channel logos and programme pictures are the exception: they
  come from wherever the provider's lists say, plain HTTP included, as its
  streams do, up to 1 MB for a logo and 4 MB for a picture, and are kept
  only when they are JPEG, PNG, WebP or GIF.
- A profile picture is decoded only up to 4096 pixels.
- NFO files are read up to 1 MiB, as XML with no DTD and none but XML's
  own entities. What they say is bounded (500 characters for a title,
  20,000 for a summary, 50 genres, 100 entries a file) and ids are checked
  for their shape. Addresses in them, pictures' included, are never
  fetched.
- Subtitles from OpenSubtitles come over HTTPS only, never redirected to
  plain HTTP.
- A file's attached fonts (for burned-in ASS subtitles) are taken up to
  256 MiB and 1,000 fonts, under names of the transcoder's own.
- Picture subtitles (PGS, DVB, VobSub), which the web app and both
  Samsung apps decode themselves, are read within bounds that real tracks
  are far inside: a track up to 100,000 samples and 512 MiB, a sample up
  to 16 MiB and 256 segments (a VobSub one up to 64 KiB and 64 control
  sequences), and for one moment up to 4,096 display sets back and a set
  amount of work, bytes read and pixels filled or drawn, each weighed by
  how long it takes; a moment that would take more isn't shown. A
  moment's pictures are at most a 4K screen's pixels, 4096 pixels a side,
  2 of them for PGS and 32 for DVB. Besides the frame just decoded, the
  frames kept decoded hold 128 MiB a track at most, the web app's
  pictures ready to draw as much again, and the native Samsung app's
  pictures leaving the screen, their copies and textures, as much again.

### Transcoder

- The transcoder Pley starts listens on `127.0.0.1` with a random token.
- One started on another computer refuses to listen on the network without
  a token of at least 16 characters, since it reads whatever files it can
  open.
- It keeps its work folder private, on Windows too, including the folders it
  makes in one chosen elsewhere (segments, live channels' too, converted
  downloads, its caches). In a work folder other accounts may write in, a
  link or a junction in place of one of those is taken away at start rather
  than followed, segments' folders are checked again before each use, and
  the caches' clean-up never prunes one that becomes a link later. A
  folder's permissions are set on the folder opened, once it is known to be
  the one there, so a link put back meanwhile is refused and what it leads
  to keeps its own.
- Segments kept in memory go in a folder of `/dev/shm` that it makes itself,
  private to its account. If another account took that name first, it
  refuses it and the segments go to disk.
- A converted download several people asked for stops only when the last of
  them, or an administrator, cancels it.

### Requests

- Radarr's and Sonarr's API keys are never shown again once saved, only
  whether one is.
- A key goes only to the address it was saved with: a new address takes its
  key again, and checking another address in Settings takes that address's
  key. Their redirects aren't followed, so a key isn't sent on elsewhere,
  nor to a link-local address, where clouds answer for the machine
  itself: neither one typed in nor one a name resolves to, checked on the
  connection itself. Their answers are read 32 MB at most.
- Only administrators approve and decline, see everyone's requests, and ask
  Radarr and Sonarr anything; others see their own.
- Discover shows TMDB's titles whatever their rating, so profiles without a
  password and accounts with an age limit may not ask until an administrator
  allows it. An account that may not ask gets none of TMDB's pictures
  through the server but those of its own requests.
- TMDB's pictures are fetched 8 at once, 10 MB each at most, and kept up
  to 256 MB in all.

### Moving from Plex

- Only administrators import, and none of the import's routes are open to
  apps.
- The Plex token, from Plex's own sign-in page or pasted, is kept in
  memory only: never saved, logged or sent to the browser, and forgotten
  when the import ends, however it ends, or the server stops. Home users' tokens
  are made from it as the import needs them, and kept no longer. Plex
  PINs given for Home users are sent to plex.tv only.
- Pley reads Plex's answers 8 MB at most each, follows no redirects, and
  sends tokens in headers, never in addresses. Home users' pictures are
  the one exception to redirects: asked for without a token, followed by
  hand three times at most and only to plex.tv, assets.plex.tv and
  secure.gravatar.com over HTTPS, taken only as an image, and squared to
  Pley's size as an uploaded picture is. It connects only to the
  server's addresses plex.tv lists, or one the administrator types, never
  to a link-local address (neither one typed in nor one a name resolves
  to), and checks that the server answering is the one chosen. It falls
  back to plain HTTP only for the server's addresses on the home network,
  never one across the internet.
- A Home user's profile is made with Plex's restrictions in the same
  step, so it's never open to everything while the import runs, or if it
  stops. A restriction Pley can't keep is made stricter, never dropped.
  A profile with labels it never sees doesn't see the libraries a run
  brings until the run finishes, since Plex's labels reach Pley's titles
  later in it: the run marks each of them (`labels_pending`) before it
  brings anything, a library it makes as it's made, and only a run that
  finishes clears the marks, so one that stops or a restart leaves them
  hidden until an administrator shows them. Profiles' own library choices
  aren't touched, and the profile can sign in meanwhile, so apps stay
  signed in. One that gets its first such label in this run has its new
  rules only once the run reaches its profile.
  Titles the import can't find on Plex don't get Plex's labels, and the
  report says so for the profiles that rely on them.
- What Plex sends is checked as any input: labels and label rules as
  Settings checks them, at most 50 rules of each kind on a profile, and
  the import's choices bounded (libraries, folders, people, PINs).
- Profiles the import makes are managed profiles with no password, as
  those made in Settings; Plex's PINs aren't copied to them.

### The desktop app

- Its window shows the server's own pages only; other sites open in the
  browser.
- It hands only web and mail links to the system, and opens finished
  downloads in the system's player or file manager.
- Server pages get none of its commands; only the pages bundled with the
  app do.
- While the window isn't in front, it follows that server's event stream
  with the window's own session, sent to that server's origin only, and
  tells of requests in the system's notifications: titles, seasons and
  names, never pictures or links.

### The Android app

- It is native and shows no web pages.
- It holds app tokens only, with a viewer's rights whoever signs in (and,
  for an administrator who allows it, approving requests): never a
  session or a password.
- Phones sign in on the server's own page in a Custom Tab, with PKCE (a
  private tab where the browser offers one, unless turned off in settings).
  TVs show a code that is typed on the server's `/link` page.
- A token answer with more than the app asked for is revoked at once and
  never used.
- Tokens are encrypted with AES-GCM under a 256-bit key that the Android
  Keystore makes and keeps, bound to their server's address, in the app's
  private storage.
- Backups and transfers to a new device are off, so tokens never leave the
  device. When the key is lost (a reset), the app is signed out. Signing out
  revokes the tokens on the server.
- Tokens go in the `Authorization` header only, and only to the server's own
  origin (scheme, host and port), checked on every hop. A redirect or a
  playlist pointing elsewhere gets none, and redirects never switch between
  HTTPS and HTTP.
- Over plain HTTP, codes and tokens go only to private addresses (loopback,
  RFC 1918, link-local, CGNAT, IPv6 unique local), checked on the connection
  itself, so a name that turns out to be on the internet is refused too. A
  server on the internet needs HTTPS.
- Only the system's certificate authorities are trusted.
- Updating from the old web view app ends its session once: the cookie it
  kept for each saved server goes to `POST /api/auth/logout` (best effort: a
  server out of reach keeps it until it expires), then the web view's
  cookies, storage and files are deleted. Nothing else in the app reads
  cookies.
- Besides its two launchers, the app exports only the sign-in's return
  address, the music service and, on TVs, the Watch Next row's pictures
  (read only, by names nobody can guess).
- The TV launcher plays a title from the Watch Next row only when the link
  carries the signature this installation gave it, so no other app can make
  it play.
- Subtitle files on the device are read only for a download's own playback,
  never because a server names one.

### The Apple app

Pley for iPhone, iPad, Mac and Apple TV (in development).

- It is native and shows no web pages.
- It holds app tokens only, with a viewer's rights whoever signs in (and,
  for an administrator who allows it, approving requests): never a
  session or a password.
- iPhone, iPad and Mac sign in on the server's own page in the system's
  sign-in sheet (`ASWebAuthenticationSession`), with PKCE (S256), a `state`
  the answer must repeat and the `iss` the server names (RFC 9207), which
  must be the server's issuer. The sheet is a private session unless
  Settings → Sign in privately is turned off: it shares no cookie with
  Safari and keeps none afterwards. Apple TV, and iPhone and iPad when
  asked, show a code that is typed on the server's `/link` page.
- The server's metadata is kept only when its issuer is exactly the address
  the app used and every endpoint is on it, and the code's page must be on
  that address too.
- A token answer with more than the app asked for is revoked at once and
  never used.
- Tokens are kept in the Keychain, readable after the first unlock, on this
  device only: never synchronised to iCloud, never in a backup restored to
  another device. A Mac build signed to run locally, without a team, keeps
  them in the login keychain instead, where "this device only" doesn't
  apply ([APPLE.md](docs/APPLE.md)). Signing out revokes them on the
  server.
- Tokens go in the `Authorization` header only, and only to the server's own
  origin (scheme, host and port). Redirects are followed only within that
  origin: never between HTTPS and HTTP, never to another host or port; the
  sign-in endpoints follow none. Downloads run in the system's background
  session, which follows redirects by itself: the server sends none for
  them. No cookies are kept or sent.
- Over plain HTTP, codes and tokens go only to private addresses (loopback,
  RFC 1918, link-local, CGNAT, IPv6 unique local). A name is looked up first
  and every address it has must be private, or nothing is sent; after the
  request, the address the connection reached must be private too. A
  sign-in whose request reached any other address, a download's included,
  is ended on the device. A server on the internet needs HTTPS. Plain HTTP
  goes straight to the server, never through the system's proxy settings,
  so a token can't reach a proxy before the address is checked; downloads,
  in the system's background session, and the player's keyed media
  addresses (which carry a key, not a token) use the system's proxy
  settings.
- Only the system's certificate authorities are trusted.
- On iPhone, iPad and Mac the app registers no URL scheme: the sign-in
  sheet catches its return address (`app.pley.apple:/oauth/callback`)
  itself. Another app could register that scheme and start a sign-in of
  its own in a browser; the server's consent page, which names the app and
  what it asks for, must still be allowed by someone signed in, and PKCE
  keeps a code from the real app's sheet for that sheet. Apple TV registers
  the scheme for its Top Shelf links, which the app signs and checks before
  opening one.
- Picture subtitles (PGS, DVB, VobSub) are bounded in what a crafted file
  makes the app hold and work through. One moment decodes a 4K screen's
  pixels and 64 pictures at most, and a DVB decode eight 4K screens of
  drawing and filling. A track keeps 128 MiB of decoded frames besides
  the one showing, and 1,024 parsed samples (64 MiB); a decode looks back
  16 MiB of samples. A sample is read for 512 segments and 16 MiB, a
  track for 100,000 samples.

### TV apps, Kodi and Roku

- They sign in with a code typed on the server's `/link` page, send tokens
  in the `Authorization` header only, and refuse plain HTTP to an address
  beyond the home network (Kodi checks the address it actually connected
  to).
- The Samsung and LG apps send the token only to the server's own origin
  (scheme, host and port), and their players get keyed media addresses,
  never the token.
- The native Samsung app keeps its tokens encrypted with AES-GCM under a
  256-bit key that the TV's key manager makes and keeps, bound to their
  server's address. Sign-in requests and server checks follow no redirect.
- The LG app's HTTP client follows no redirect by itself: the app decides
  each hop (a collection's picture), so a token never follows one
  elsewhere. It keeps each server's tokens in its data folder on the TV,
  which only the app's own user can read (folders 0700, files 0600),
  never in its install folder, and writes each file whole (a temporary
  file, then a rename). Refreshes run one at a time and the new pair is
  saved before it is used. A server that grants more than the app asked
  for has the grant revoked and the TV signed out. Signing out revokes the
  tokens on the server; while the server can't prove itself at its
  address, the TV only forgets them, and the server's Account → Apps ends
  the sign-in.
- The LG app carries its own TLS (rustls) and root certificates, since a
  TV's own are as old as its firmware. Its log on the TV
  (`/tmp/app.pley.tv/pley.log`, readable by the app's group) leaves out
  tokens, codes, keyed addresses, the Live TV relay's path and a USB
  drive's folder and file names.
- The LG app hands a live channel's MPEG-TS stream to the TV's player,
  which takes no request headers, through a relay of its own: it reads
  the stream with the token and serves it on 127.0.0.1 alone, under a
  path of 128 random bits made for that channel, to at most four
  connections, each sending its request's head (8 KiB at most) within
  5 s. The relay serves that one stream and closes it, its connections
  with it, when the channel changes or the player goes.
- Kodi's player and artwork cache reach the server through the add-on's
  proxy on 127.0.0.1, behind a random path checked in constant time, for
  requests addressed to it alone.
