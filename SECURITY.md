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
  another computer, and the Jellyfin-compatible API's own port (such as
  8096) if you gave it one: on Pley's port it is behind your reverse proxy's
  HTTPS like the rest.
- **Leave the Jellyfin-compatible API off unless an app needs it.** It is
  off by default; **Account → Tokens for other apps** shows each app signed
  in through it, and revokes it.
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
  administrator, a higher age limit, other libraries) takes that profile's
  PIN, or its password (and code).

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
- Apps never get administrator rights. Every route they may use is listed
  with the scope it needs, anything else is refused, and the
  Jellyfin-compatible API refuses their tokens.
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

### Apps using the Jellyfin-compatible API

Jellyfin's apps, and other apps and scripts built on its API, use tokens for
other apps (`pley_ct_…`), never a session's
([COMPATIBILITY.md](docs/COMPATIBILITY.md)).

- The API is off until an administrator turns it on. On Pley's port, only
  paths it alone has, or requests carrying a Jellyfin app's header or token,
  reach it; `/api/…` never does. Its answers are never pages (a sandboxing
  `Content-Security-Policy`, `nosniff`, no framing).
- An app gets a token by signing in with an account's password, which goes
  through the checks of Pley's own sign-in (attempt limits, disabled
  accounts) and makes no session. An account with two-step sign-in can't
  sign in this way, nor one without it while the server requires it: the
  API has no second step. People can also make one on the account page,
  which shows it once. Tokens are random 256-bit values, stored hashed.
- Tokens work on this API only, from Jellyfin's headers or `api_key`, never
  from the cookie. Pley's own API refuses them wherever they are sent, and
  this API refuses sessions' and apps' tokens.
- A token sees what its account may see, for that account alone, and never
  acts as an administrator: nothing administers through this API. It lists
  no one but its user (`/Users/Public` is empty), and a user id in a path
  must be its own.
- Ids are keyed with the server's `secret.key` (an HMAC), so they can't be
  guessed and another server's mean nothing. Players open streams,
  subtitles and pictures without a token: a media source's id stands for
  the sign-in it was listed to and works only while that sign-in does, for
  what its user may see; HLS sessions started through this API are reached
  by their 128-bit id while the token that started them works; a picture
  answers for an id someone was given, and with a
  token only for what its user may see. Files are shown by their names,
  never where the server keeps them.
- A token ends when it is revoked, after 90 days unused, and with the
  account's other sign-ins: a new password, **Sign out everywhere**,
  `pley reset-password` and deleting the account. It doesn't work while the
  account is disabled, nor while two-step sign-in is required of an account
  that doesn't use it. A revoked token stops working at once, even one being
  checked at that moment.
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
  web port, which its public endpoint tells anyone who reaches it.
- The UDP answer goes only to addresses on the local network, at most 100
  a second. The Bonjour announcement goes out only from adapters with
  addresses on the local network, never to the internet.
- `--no-discovery` (`PLEY_NO_DISCOVERY`), or Settings → Network, turns
  both off; the Docker setup has them off.

### Files

- A symbolic link in a library is followed only to a file of the same kind
  (a video to a video, a picture to a picture). This is checked when the
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
- Subtitles from OpenSubtitles come over HTTPS only, never redirected to
  plain HTTP.
- A file's attached fonts (for burned-in ASS subtitles) are taken up to
  256 MiB and 1,000 fonts, under names of the transcoder's own.

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
- It holds app tokens only, with a viewer's rights whoever signs in: never a
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

### TV apps, Kodi and Roku

- They sign in with a code typed on the server's `/link` page, send tokens
  in the `Authorization` header only, and refuse plain HTTP to an address
  beyond the home network (Kodi checks the address it actually connected
  to).
- The Samsung and LG apps send the token only to the server's own origin.
- The native Samsung app keeps its tokens encrypted with AES-GCM under a
  256-bit key that the TV's key manager makes and keeps, bound to their
  server's address. Sign-in requests and server checks follow no redirect.
- Kodi's player and artwork cache reach the server through the add-on's
  proxy on 127.0.0.1, behind a random path checked in constant time, for
  requests addressed to it alone.
