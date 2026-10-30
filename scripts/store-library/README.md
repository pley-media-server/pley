# The store screenshots' library

A made-up library for the [store screenshots](../../deploy/stores/README.md),
so no real title's artwork appears: twelve films and four shows with their
own posters, backdrops, stills, plots, genres and ratings. Each title is a
video of its picture (1080p, silent 5.1), quick to make.

```sh
bun scripts/store-library/art.mjs <dir>           # the pictures, through a headless Chrome
bun scripts/store-library/media.mjs <dir>         # Movies/ and Shows/, and the videos to make
tr '\n' '\0' < <dir>/jobs.txt | xargs -0 -P 6 -I{} bash scripts/store-library/encode.sh "{}"
```

Then start a server of its own, with a new data folder and no internet, so
the metadata providers can't match the made-up titles to real ones: set
`HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` to a closed port, and
`NO_PROXY=127.0.0.1,localhost`. Set it up with the code from its log:

```sh
pley serve --data-dir <dir>/data --bind 127.0.0.1:17560 --plex-bind off --no-discovery --no-tray
bun scripts/store-library/setup.mjs http://127.0.0.1:17560 <setup code> <dir>
```

`setup.mjs` makes the account (Alex; the password is in `<dir>/admin.json`),
the libraries and their metadata, something to resume, things watched, and
My List.

- `bun scripts/store-library/web.mjs <dir> http://127.0.0.1:17560` takes the
  README's pictures of the web app (`docs/images`): five screenshots and the
  tour GIF (it needs `ffmpeg` on `PATH`).
- Emulators reach the server at `10.0.2.2:17560`.
- Take phone screenshots at 1080 × 1920 (`adb shell wm size 1080x1920`):
  Google Play takes nothing longer than 2:1.
