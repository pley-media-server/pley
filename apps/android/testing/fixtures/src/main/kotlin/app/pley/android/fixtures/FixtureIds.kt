package app.pley.android.fixtures

/**
 * What tests, demos and benchmarks can count on in the FixtureServer's
 * catalog, which is the same on every run.
 *
 * Three libraries: [MOVIES] (1,500 movies), [SHOWS] (300 shows with their
 * seasons and episodes) and [MUSIC] (50 artists, 200 albums). Most titles
 * are made up in bulk with realistic lengths, and their files stream one of
 * the two committed clips. The items named here play exactly what their
 * files say: 20 s clips whose durations, streams and chapters are the real
 * ones.
 */
object FixtureIds {
    // ---------------------------------------------------------------- libraries
    const val MOVIES: Long = 1
    const val SHOWS: Long = 2
    const val MUSIC: Long = 3

    // ---------------------------------------------------------------- movies
    /** 1080p24 H.264 and stereo AAC in MP4, played directly by most devices. */
    const val SAMPLE_1080P: Long = 1
    const val SAMPLE_1080P_FILE: Long = 1_000_001

    /** 2160p23.976 HEVC Main 10 HDR10, AC-3 5.1, an English SRT track and three chapters in MKV. */
    const val SAMPLE_HDR10: Long = 2
    const val SAMPLE_HDR10_FILE: Long = 1_000_002

    /** The HDR10 sample's embedded English subtitles: partial (`X-Pley-Complete: 0`) the first time, whole after. */
    const val SAMPLE_HDR10_SUBTITLE: Int = 2

    /** Its French subtitle file beside the media. */
    const val SAMPLE_HDR10_EXTERNAL_SUBTITLE: Int = 1000

    /** Dolby Vision profile 5 (no compatible base layer), for playback-policy tests; streams the HDR10 clip. */
    const val DOLBY_VISION_P5: Long = 3

    /** Dolby Vision profile 7 (dual layer, HDR10 base layer). */
    const val DOLBY_VISION_P7: Long = 4

    /** Dolby Vision profile 8.1 (HDR10 base layer). */
    const val DOLBY_VISION_P81: Long = 5

    /** HDR10+ dynamic metadata. */
    const val HDR10_PLUS: Long = 6

    // ---------------------------------------------------------------- shows
    /** "Fixture Show": two seasons of three 20 s episodes with an intro and credits each. */
    const val SHOW: Long = 10_001
    const val SHOW_SEASON_1: Long = 11_001
    const val SHOW_SEASON_2: Long = 11_002
    const val SHOW_S1E1: Long = 20_001
    const val SHOW_S1E2: Long = 20_002
    const val SHOW_S1E3: Long = 20_003
    const val SHOW_S2E1: Long = 20_004

    /** Where the fixture show's episodes' intro starts and ends, in ms. */
    const val INTRO_START_MS: Long = 2_000
    const val INTRO_END_MS: Long = 8_000

    /** Where their credits start; they run to the end. */
    const val CREDITS_START_MS: Long = 16_000

    // ---------------------------------------------------------------- music
    const val ARTIST: Long = 60_001

    /** The first album: its tracks are 20 s, as their file. */
    const val ALBUM: Long = 61_001
    const val TRACK: Long = 62_001

    // ---------------------------------------------------------------- collections, playlists
    /** A smart collection of the movies library: its comedies of the 1990s. */
    const val SMART_COLLECTION: Long = 1
    const val COLLECTION: Long = 2

    /** Alex's playlists: movies, tracks, and a smart one (the library's unwatched comedies). */
    const val PLAYLIST_VIDEO: Long = 1
    const val PLAYLIST_AUDIO: Long = 2
    const val PLAYLIST_SMART: Long = 3

    // ---------------------------------------------------------------- profiles
    /** The household. Signing in acts as Alex unless told otherwise. */
    const val ALEX: Long = 1

    /** Has the PIN [PIN]. */
    const val SAM: Long = 2

    /** Needs a password: switching to Robin means signing in again in the browser. */
    const val ROBIN: Long = 3

    /** Sam's PIN. */
    const val PIN: String = "1234"
}
