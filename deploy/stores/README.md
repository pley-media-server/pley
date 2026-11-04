# Store listings

What Google Play and the Amazon Appstore ask for besides the Android app
itself: pictures, screenshots and the listing's text.

## Pictures

`bun scripts/render-icons.mjs` renders these from the logo
(`apps/web/public/favicon.svg`), along with every other icon.

| File | Size | For |
|---|---|---|
| `icon-512.png` | 512 × 512 | Google Play's app icon (Play rounds its corners), the Amazon Appstore's large icon |
| `icon-114.png` | 114 × 114 | The Amazon Appstore's small icon |
| `feature-graphic-1024x500.png` | 1024 × 500 | Google Play's feature graphic |
| `tv-banner-1280x720.png` | 1280 × 720 | Google Play's Android TV banner |
| `fire-tv-icon-1280x720.png` | 1280 × 720 | The Amazon Appstore's Fire TV icon, the app's tile on Fire TV: the mark and name centred, larger than the banner's |
| `fire-tv-background-1920x1080.jpg` | 1920 × 1080 | The Amazon Appstore's Fire TV background (the mark top right, clear of the store's text) |

## Screenshots

`screenshots/` holds JPEGs as Google Play takes them: phones at
1080 × 1920 (the player at 1920 × 1080), TVs at 1920 × 1080. They show a
made-up library, which [scripts/store-library](../../scripts/store-library/README.md)
recreates.

| Phone | TV |
|---|---|
| `phone-1-home` Home: Continue Watching, My List | `tv-1-home` Home |
| `phone-2-movie` A film's page | `tv-2-library` The Movies library |
| `phone-3-library` The Movies library | `tv-3-movie` A film's page |
| `phone-4-show` A show's page | `tv-4-episodes` A season's episodes |
| `phone-5-episodes` A season's episodes, with downloads | `tv-5-player` The player |
| `phone-6-player` The player | |
| `phone-7-downloads` Downloads, for watching offline | |

## The app

- **Package**: `com.formandelement.pley`, on Google Play and the Amazon
  Appstore (Fire TV and Fire tablets only: the Appstore takes no other
  Android phones and tablets).
- **Upload**: the release's bundle, `pley-android-<version>-play.aab`
  ([ANDROID.md](../../docs/ANDROID.md#releases)).
- **Version code**: derived from the version
  ([RELEASING.md](../../docs/RELEASING.md#androids-version-code)). The stores
  take each code once. To upload the same version again, build it with
  `PLEY_BUILD` set (1 to 99; `PLEY_BUILD=1` makes 10000101) and the release
  key: `./gradlew :app:bundleStoreRelease` in `apps/android`.
- **Category**: Video Players & Editors.
- **Name**: Pley

**Short description** (80 characters at most):

> Your own movies, shows and music, from your Pley server, on phones and TVs.

**Full description**:

> Pley plays the movies, shows and music on your own Pley media server, at
> home or away.
>
> - Continue Watching, My List and every library, with posters, plots and
>   episode stills.
> - Plays files as they are whenever the device can, in its own player;
>   your server converts the rest.
> - HDR and Dolby Vision on the TVs that show them.
> - Skip Intro and Up Next.
> - Downloads for watching offline, which report what you watched when
>   you're back.
> - One app for phones, tablets, Android TV, Google TV and Fire TV.
> - Signs in with a code shown on the TV, typed on your phone or computer.
>
> Pley needs a Pley server, which runs on Windows, macOS, Linux and Docker.
