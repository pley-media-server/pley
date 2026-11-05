package app.pley.android.model.format

import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.JobState
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.MediaSummary
import app.pley.android.model.StreamAction
import app.pley.android.model.StreamKind
import app.pley.android.model.TranscodeInfo
import app.pley.android.model.UserState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.ZoneOffset
import java.util.Locale

/** The cases of apps/web/src/lib/format.test.ts, plus the native screens' own wording. */
class FormatTest {
    private val us = Locale.US

    private fun item(
        kind: ItemKind = ItemKind.Movie,
        title: String = "Heat",
        year: Int? = null,
        index: Int? = null,
        parentIndex: Int? = null,
        parentTitle: String? = null,
        grandparentTitle: String? = null,
        childCount: Int? = null,
        leafCount: Int? = null,
        viewedLeafCount: Int? = null,
        durationMs: Long? = null,
        userState: UserState = UserState(),
    ) = ItemSummary(
        id = 1, libraryId = 1, kind = kind, title = title, year = year, index = index, parentIndex = parentIndex,
        parentTitle = parentTitle, grandparentTitle = grandparentTitle, childCount = childCount, leafCount = leafCount,
        viewedLeafCount = viewedLeafCount, durationMs = durationMs, userState = userState,
    )

    private fun stream(
        kind: StreamKind = StreamKind.Video,
        codec: String = "h264",
        language: String? = null,
        title: String? = null,
        channels: Int? = null,
        forced: Boolean = false,
        external: Boolean = false,
        default: Boolean = false,
        width: Int? = null,
        height: Int? = null,
        bitDepth: Int? = null,
        range: DynamicRange? = null,
        dvProfile: Int? = null,
        dvBlCompat: Int? = null,
        profile: String? = null,
    ) = MediaStream(
        index = 0, kind = kind, codec = codec, language = language, title = title, channels = channels, forced = forced,
        external = external, default = default, width = width, height = height, bitDepth = bitDepth,
        dynamicRange = range, dvProfile = dvProfile, dvBlCompat = dvBlCompat, profile = profile,
    )

    @Test
    fun durations() {
        assertEquals("1h 52m", formatDuration(6_720_000))
        assertEquals("45m", formatDuration(2_700_000))
        assertEquals("1h", formatDuration(3_600_000))
        assertEquals("2h", formatDuration(7_170_000))
        assertEquals("1m", formatDuration(59_000))
        assertEquals("29s", formatDuration(29_000))
        assertEquals("1s", formatDuration(400))
        assertEquals("", formatDuration(0))
        assertEquals("", formatDuration(null))
        assertEquals("", formatDuration(-5_000))
    }

    @Test
    fun clocks() {
        assertEquals("1:02:03", formatClock(3723.0))
        assertEquals("2:03", formatClock(123.0))
        assertEquals("10:00:00", formatClock(36_000.0))
        assertEquals("0:59", formatClock(59.9))
        for (value in listOf(null, Double.NaN, Double.POSITIVE_INFINITY, -1.0, 0.0)) assertEquals("0:00", formatClock(value))
        assertEquals("1:02:03", formatClockMs(3_723_400))
    }

    @Test
    fun remaining() {
        assertEquals("34m left", formatRemaining(600_000, 2_640_000))
        assertEquals("", formatRemaining(5_000, null))
        assertEquals("", formatRemaining(2_640_000, 2_640_000))
        assertEquals("", formatRemaining(3_000_000, 2_640_000))
    }

    @Test
    fun runtimesAndTimeLeftAsTheScreensSayThem() {
        assertEquals("58 min", formatRuntime(3_480_000))
        assertEquals("1h 52m", formatRuntime(6_720_000))
        assertEquals("2h", formatRuntime(7_200_000))
        assertEquals("29s", formatRuntime(29_000))
        assertEquals("", formatRuntime(null))
        assertEquals("26 min left", formatTimeLeft(1_920_000, 3_480_000))
        assertEquals("1h 12m left", formatTimeLeft(3_000_000, 7_320_000))
        assertEquals("", formatTimeLeft(7_320_000, 7_320_000))
    }

    @Test
    fun spokenDurations() {
        assertEquals("34 minutes", spokenDuration(2_040_000))
        assertEquals("1 hour 12 minutes", spokenDuration(4_320_000))
        assertEquals("2 hours", spokenDuration(7_200_000))
        assertEquals("1 minute", spokenDuration(60_000))
        assertEquals("29 seconds", spokenDuration(29_000))
        assertEquals("34 minutes left", spokenTimeLeft(600_000, 2_640_000))
    }

    @Test
    fun bytesAndBitrates() {
        assertEquals("0 B", formatBytes(0))
        assertEquals("512 B", formatBytes(512))
        assertEquals("1.0 KB", formatBytes(1024))
        assertEquals("1.5 KB", formatBytes(1536))
        assertEquals("150 MB", formatBytes(150L * 1024 * 1024))
        assertEquals("5.0 TB", formatBytes(5L * 1024 * 1024 * 1024 * 1024))
        assertEquals("1024 TB", formatBytes(1024L * 1024 * 1024 * 1024 * 1024))
        assertEquals("", formatBytes(null))
        assertEquals("8.2 Mbps", formatBitrate(8_200_000))
        assertEquals("12 Mbps", formatBitrate(12_400_000))
        assertEquals("320 kbps", formatBitrate(320_000))
        assertEquals("", formatBitrate(null))
    }

    @Test
    fun relativeTimes() {
        val now = 1_700_000_000L
        assertEquals("30 seconds ago", formatRelative(now - 30, now))
        assertEquals("3 minutes ago", formatRelative(now - 180, now))
        assertEquals("2 hours ago", formatRelative(now - 7200, now))
        assertEquals("3 days ago", formatRelative(now - 3 * 86_400, now))
        assertEquals("2 weeks ago", formatRelative(now - 14 * 86_400, now))
        assertEquals("2 months ago", formatRelative(now - 60 * 86_400, now))
        assertEquals("2 years ago", formatRelative(now - 2 * 365 * 86_400, now))
        assertEquals("in 5 hours", formatRelative(now + 5 * 3600, now))
        assertEquals("yesterday", formatRelative(now - 86_400, now))
        assertEquals("last week", formatRelative(now - 7 * 86_400, now))
        assertEquals("now", formatRelative(now, now))
        assertEquals("1 minute ago", formatRelative(now - 60, now))
        assertEquals("never", formatRelative(null, now))
        assertEquals("never", formatRelative(0, now))
    }

    @Test
    fun dates() {
        assertEquals("Apr 1, 2023", formatDate("2023-04-01", us))
        assertEquals("sometime", formatDate("sometime", us))
        assertEquals("", formatDate(null, us))
        assertEquals("12 March 2024", formatLongDate("2024-03-12", Locale.UK))
        assertEquals("Oct 24, 2026", formatDay(1_792_800_000, us, ZoneOffset.UTC))
        assertEquals("", formatDay(null, us))
    }

    @Test
    fun uptime() {
        assertEquals("1d 1h", formatUptime(90_061))
        assertEquals("1h 1m", formatUptime(3_700))
        assertEquals("2m", formatUptime(120))
        assertEquals("0m", formatUptime(59))
    }

    @Test
    fun plurals() {
        assertEquals("1 movie", pluralize(1, "movie", locale = us))
        assertEquals("0 movies", pluralize(0, "movie", locale = us))
        assertEquals("2 movies", pluralize(2, "movie", locale = us))
        assertEquals("3 children", pluralize(3, "child", "children", us))
        assertEquals("12,345 movies", pluralize(12_345, "movie", locale = us))
    }

    @Test
    fun episodeAndSeasonLabels() {
        assertEquals("S1 · E3", episodeLabel(1, 3))
        assertEquals("E3", episodeLabel(null, 3))
        assertEquals("S0 · E0", episodeLabel(0, 0))
        assertEquals("", episodeLabel(null, null))
        assertEquals("The Final Season", seasonTitle("The Final Season", 9))
        assertEquals("Specials", seasonTitle("", 0))
        assertEquals("Season 2", seasonTitle("", 2))
        assertEquals("Season", seasonTitle("", null))
        assertEquals("Season 2 · Episode 5", episodeLongLabel(2, 5))
    }

    @Test
    fun resolutions() {
        assertEquals("4K", resolutionLabel(3840, 2160))
        assertEquals("4K", resolutionLabel(3840, 1600))
        assertEquals("1440p", resolutionLabel(2560, 1440))
        assertEquals("1080p", resolutionLabel(1920, 800))
        assertEquals("720p", resolutionLabel(1280, 720))
        assertEquals("576p", resolutionLabel(720, 576))
        assertEquals("480p", resolutionLabel(720, 480))
        assertEquals("SD", resolutionLabel(640, 360))
        assertNull(resolutionLabel(null, null))
        assertNull(resolutionLabel(0, 0))
    }

    @Test
    fun codecsAndChannels() {
        assertEquals("H.264", codecLabel("h264"))
        assertEquals("HEVC", codecLabel("HEVC"))
        assertEquals("PGS", codecLabel("hdmv_pgs_subtitle"))
        assertEquals("CINEPAK", codecLabel("cinepak"))
        assertEquals("", codecLabel(null))
        assertEquals("Mono", channelsLabel(1))
        assertEquals("Stereo", channelsLabel(2))
        assertEquals("5.1", channelsLabel(6))
        assertEquals("7.1", channelsLabel(8))
        assertEquals("5ch", channelsLabel(5))
        assertEquals("quad", channelsLabel(null, "quad"))
        assertEquals("", channelsLabel(0))
    }

    @Test
    fun streamLabels() {
        assertEquals("English (EAC3 5.1)", streamLabel(stream(StreamKind.Audio, "eac3", "eng", channels = 6)))
        assertEquals("English (TrueHD 7.1)", streamLabel(stream(StreamKind.Audio, "truehd", "eng", channels = 8)))
        assertEquals(
            "English (TrueHD Atmos 7.1)",
            streamLabel(stream(StreamKind.Audio, "truehd", "eng", channels = 8, profile = "Dolby TrueHD + Dolby Atmos")),
        )
        assertEquals(
            "English – Commentary (AAC Stereo)",
            streamLabel(stream(StreamKind.Audio, "aac", "eng", "Commentary", channels = 2)),
        )
        assertEquals("Director's track (AAC Stereo)", streamLabel(stream(StreamKind.Audio, "aac", title = "Director's track", channels = 2)))
        assertEquals("English (AC3 5.1)", streamLabel(stream(StreamKind.Audio, "ac3", "eng", "English 5.1", channels = 6)))
        assertEquals("Unknown (Opus)", streamLabel(stream(StreamKind.Audio, "opus")))
        assertEquals(
            "French (SRT, Forced, External)",
            streamLabel(stream(StreamKind.Subtitle, "subrip", "fre", forced = true, external = true)),
        )
        assertEquals("French – SDH (SRT, Forced)", streamLabel(stream(StreamKind.Subtitle, "subrip", "fre", "SDH", forced = true)))
        assertEquals("Portuguese (BR) – SDH (PGS)", streamLabel(stream(StreamKind.Subtitle, "hdmv_pgs_subtitle", "pt-BR", "SDH")))
        // Only named forced, as the server counts them: told apart from the file's full English.
        assertEquals("English (SRT)", streamLabel(stream(StreamKind.Subtitle, "subrip", "eng", "English")))
        assertEquals("English (SRT, Forced)", streamLabel(stream(StreamKind.Subtitle, "subrip", "eng", "English (Forced)")))
        assertEquals("English – Signs & Songs (SRT, Forced)", streamLabel(stream(StreamKind.Subtitle, "subrip", "eng", "Signs & Songs")))
        assertEquals("English – Unforced Errors (SRT)", streamLabel(stream(StreamKind.Subtitle, "subrip", "eng", "Unforced Errors")))
        val dv = stream(codec = "hevc", width = 3840, height = 2160, range = DynamicRange.DolbyVision, dvProfile = 8, dvBlCompat = 1, bitDepth = 10)
        assertEquals("4K HEVC Dolby Vision P8.1 10-bit", streamLabel(dv))
        assertEquals("1080p H.264", streamLabel(stream(width = 1920, height = 1080, bitDepth = 8, range = DynamicRange.Sdr)))
    }

    @Test
    fun dynamicRanges() {
        assertEquals("Dolby Vision", dynamicRangeLabel(DynamicRange.DolbyVision))
        assertEquals("Dolby Vision P5", dynamicRangeLabel(DynamicRange.DolbyVision, 5, 0))
        assertEquals("Dolby Vision P8.4", dynamicRangeLabel(DynamicRange.DolbyVision, 8, 4))
        assertEquals("HDR10+", dynamicRangeLabel(DynamicRange.Hdr10Plus))
        assertEquals("HDR10", dynamicRangeLabel(DynamicRange.Hdr10))
        assertEquals("HLG", dynamicRangeLabel(DynamicRange.Hlg))
        assertNull(dynamicRangeLabel(DynamicRange.Sdr, hdr = true))
        assertEquals("HDR", dynamicRangeLabel(null, hdr = true))
        assertNull(dynamicRangeLabel(null, hdr = false))
        assertNull(dynamicRangeLabel(null as MediaStream?))
        // Profile 7's base layer (Blu-ray, compatibility 6) isn't named: never "P7.6".
        assertEquals("Dolby Vision P7", dynamicRangeLabel(DynamicRange.DolbyVision, 7, 6))
        assertEquals("Dolby Vision P7 FEL", dynamicRangeLabel(DynamicRange.DolbyVision, 7, 6, dvEl = "fel"))
        assertEquals("Dolby Vision P7 MEL", dynamicRangeLabel(DynamicRange.DolbyVision, 7, 6, dvEl = "mel"))
        assertEquals("Dolby Vision P8.1 + HDR10+", dynamicRangeLabel(DynamicRange.DolbyVision, 8, 1, hdr10plus = true))
        assertEquals("DV P8.1", dynamicRangeShort(DynamicRange.DolbyVision, 8, 1))
        assertEquals("DV P7", dynamicRangeShort(DynamicRange.DolbyVision, 7, 6))
        assertEquals("DV P7 FEL", dynamicRangeShort(DynamicRange.DolbyVision, 7, 6, dvEl = "fel"))
        assertEquals("DV", dynamicRangeShort(DynamicRange.DolbyVision))
        assertEquals("HDR", dynamicRangeShort(null, hdr = true))
        assertNull(dynamicRangeShort(DynamicRange.Sdr))
    }

    @Test
    fun dolbyVisionWithHdr10PlusTakesBothBadges() {
        val hybrid = stream(range = DynamicRange.DolbyVision, dvProfile = 8, dvBlCompat = 1).copy(hdr10plus = true)
        assertEquals(listOf("DV P8.1", "HDR10+"), dynamicRangeBadges(hybrid))
        assertEquals("Dolby Vision P8.1 + HDR10+", dynamicRangeLabel(hybrid))
        assertEquals(listOf("DV P8.1"), dynamicRangeBadges(hybrid.copy(hdr10plus = false)))
        assertEquals(listOf("HDR10+"), dynamicRangeBadges(stream(range = DynamicRange.Hdr10Plus).copy(hdr10plus = true)))
        assertEquals(emptyList<String>(), dynamicRangeBadges(null))
    }

    @Test
    fun qualityBadgesOfAFile() {
        val file = MediaFile(
            id = 1, path = "Ashfall.mkv", size = 66_571_993_088, container = "mkv", width = 3840, height = 2160, videoCodec = "hevc",
            streams = listOf(
                stream(codec = "hevc", width = 3840, height = 2160, range = DynamicRange.DolbyVision, dvProfile = 8, dvBlCompat = 1),
                stream(StreamKind.Audio, "truehd", "eng", channels = 8, default = true, profile = "Dolby TrueHD + Dolby Atmos"),
            ),
        )
        assertEquals(listOf("4K", "DV P8.1", "Atmos"), qualityBadges(file))
        assertEquals("4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB", versionDescription(file))
        assertEquals("4K · HEVC · MKV · 62.0 GB", versionLabel(file, 0))
        val plain = MediaFile(
            id = 2, path = "x.mp4", width = 1920, height = 1080,
            streams = listOf(stream(range = DynamicRange.Hdr10Plus), stream(StreamKind.Audio, "eac3", channels = 6)),
        )
        assertEquals(listOf("1080p", "HDR10+", "5.1"), qualityBadges(plain))
        assertEquals(emptyList<String>(), qualityBadges(null as MediaFile?))
        assertEquals("Version 3", versionLabel(MediaFile(id = 3, path = "y"), 2))
        assertEquals(listOf("4K", "DV"), qualityBadges(MediaSummary(width = 3840, height = 2160, dynamicRange = DynamicRange.DolbyVision)))
    }

    @Test
    fun mediaSummaries() {
        val media = MediaSummary(42_000_000, 25_100_000_000, 3840, 2160, "hevc", DynamicRange.DolbyVision)
        assertEquals(listOf("4K", "DV"), mediaBadges(media))
        assertEquals("42 Mbps · 23.4 GB", mediaLine(media))
        assertEquals(emptyList<String>(), mediaBadges(media.copy(width = 1920, height = 1080, dynamicRange = DynamicRange.Sdr, videoCodec = "h264")))
        assertEquals(listOf("HDR"), mediaBadges(media.copy(width = 1920, height = 1080, dynamicRange = DynamicRange.Hdr10)))
        assertEquals(emptyList<String>(), mediaBadges(MediaSummary()))
        assertEquals("Bitrate unknown", mediaLine(MediaSummary()))
        assertEquals("", mediaLine(null))
    }

    @Test
    fun transcodeSummaries() {
        val base = TranscodeInfo(
            jobId = "j1", state = JobState.Unknown, video = StreamAction.Transcode, audio = StreamAction.Copy,
            videoIn = "HEVC 3840×2160 Dolby Vision P8.1", videoOut = "H.264 1920×1080", audioIn = "EAC3 5.1",
            encoder = "h264_nvenc", decoder = "cuda", tonemap = "libplacebo", speed = 3.21,
        )
        assertEquals(
            "HEVC 3840×2160 Dolby Vision P8.1 → H.264 1920×1080 · EAC3 5.1 (copy) · h264_nvenc · cuda decode · libplacebo · 3.2×",
            transcodeSummary(base),
        )
        assertEquals(
            "Video (copy) · TrueHD 7.1 → AAC Stereo · burn-in subs",
            transcodeSummary(
                base.copy(
                    video = StreamAction.Copy, videoIn = null, audio = StreamAction.Transcode, audioIn = "TrueHD 7.1",
                    audioOut = "AAC Stereo", decoder = "cpu", tonemap = null, subtitleBurn = true, speed = null,
                ),
            ),
        )
        assertEquals(
            "Video transcode · h264_nvenc",
            transcodeSummary(base.copy(videoIn = null, videoOut = null, audio = StreamAction.None, decoder = null, tonemap = null, speed = 0.0)),
        )
    }

    @Test
    fun cardSubtitles() {
        assertEquals("1999", cardSubtitle(item(year = 1999), FormatWords.English, us))
        assertEquals("", cardSubtitle(item(), FormatWords.English, us))
        assertEquals("3 seasons", cardSubtitle(item(ItemKind.Show, childCount = 3, leafCount = 30), FormatWords.English, us))
        assertEquals("1 episode", cardSubtitle(item(ItemKind.Show, childCount = 0, leafCount = 1), FormatWords.English, us))
        assertEquals("2005", cardSubtitle(item(ItemKind.Show, year = 2005), FormatWords.English, us))
        assertEquals("10 episodes", cardSubtitle(item(ItemKind.Season, leafCount = 10), FormatWords.English, us))
        assertEquals("S2 · E5", cardSubtitle(item(ItemKind.Episode, parentIndex = 2, index = 5), FormatWords.English, us))
        assertEquals("1 album", cardSubtitle(item(ItemKind.Artist, childCount = 1), FormatWords.English, us))
        assertEquals("Daft Punk · 2001", cardSubtitle(item(ItemKind.Album, parentTitle = "Daft Punk", year = 2001), FormatWords.English, us))
        assertEquals("Daft Punk · Discovery", cardSubtitle(item(ItemKind.Track, grandparentTitle = "Daft Punk", parentTitle = "Discovery"), FormatWords.English, us))
        assertEquals("", cardSubtitle(item(ItemKind.Extra), FormatWords.English, us))
    }

    @Test
    fun fullTitles() {
        assertEquals(
            "The Office — S2 · E3 — Office Olympics",
            fullTitle(item(ItemKind.Episode, "Office Olympics", grandparentTitle = "The Office", parentIndex = 2, index = 3)),
        )
        assertEquals("The Office — Season 2", fullTitle(item(ItemKind.Season, "", parentTitle = "The Office", index = 2)))
        assertEquals("One More Time — Daft Punk", fullTitle(item(ItemKind.Track, "One More Time", grandparentTitle = "Daft Punk")))
        assertEquals("Heat (1995)", fullTitle(item(year = 1995)))
        assertEquals("Heat", fullTitle(item()))
    }

    @Test
    fun watchProgress() {
        assertEquals(0.25f, progressFraction(30_000, 120_000))
        assertEquals(1f, progressFraction(200_000, 120_000))
        assertEquals(0f, progressFraction(30_000, null))
        assertEquals(0f, progressFraction(0, 120_000))
        assertEquals(6, unwatchedCount(item(ItemKind.Show, leafCount = 10, viewedLeafCount = 4)))
        assertEquals(10, unwatchedCount(item(ItemKind.Show, leafCount = 10)))
        assertEquals(0, unwatchedCount(item(ItemKind.Show, leafCount = 3, viewedLeafCount = 5)))
        assertEquals(0, unwatchedCount(item()))
        assertTrue(isPlayableVideo(ItemKind.Movie))
        assertTrue(isPlayableVideo(ItemKind.Episode))
        assertTrue(isPlayableVideo(ItemKind.Extra))
        assertFalse(isPlayableVideo(ItemKind.Show))
        assertFalse(isPlayableVideo(ItemKind.Track))
    }

    @Test
    fun tileDescriptions() {
        val episode = item(
            ItemKind.Episode, "Glasshouse", grandparentTitle = "Driftline", parentIndex = 2, index = 5, durationMs = 3_480_000,
            userState = UserState(positionMs = 1_920_000),
        )
        assertEquals("Glasshouse, Driftline, Season 2 Episode 5, 26 minutes left", tileDescription(episode, FormatWords.English, us))
        assertEquals("Northern Lights, 2024, watched", tileDescription(item(title = "Northern Lights", year = 2024, userState = UserState(watched = true)), FormatWords.English, us))
        assertEquals(
            "Severance, 2 seasons, 6 unwatched",
            tileDescription(item(ItemKind.Show, "Severance", childCount = 2, leafCount = 19, viewedLeafCount = 13), FormatWords.English, us),
        )
    }

    @Test
    fun languages() {
        assertEquals("English", languageName("eng"))
        assertEquals("Portuguese (BR)", languageName("pt-BR"))
        assertEquals("XYZ", languageName("xyz"))
        assertEquals("klingon", languageName("klingon"))
        assertNull(languageName(" "))
        assertNull(languageName(null))
    }
}
