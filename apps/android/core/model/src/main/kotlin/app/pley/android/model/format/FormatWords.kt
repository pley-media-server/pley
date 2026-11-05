package app.pley.android.model.format

import java.util.Locale

/**
 * The words the format functions write, in one language: units, what is
 * left, seasons and episodes, relative times, a stream's details. Each
 * function takes them last and writes [English] when given none, as it
 * always has; the app's, in the language shown, come from its resources
 * (formatWords in :core:common, and in Compose in :core:designsystem).
 * Numbers and the names of languages follow [locale].
 */
interface FormatWords {
    /** The locale these words are in, for numbers and the names of languages. */
    val locale: Locale

    /** What separates a number's decimals: "1.5" in English, "1,5" in French. */
    val decimalSeparator: Char

    /** "29s", "45m", "1h", "1h 52m", "1d 1h": durations, short. */
    fun seconds(n: Long): String
    fun minutes(n: Long): String
    fun hours(n: Long): String
    fun hoursMinutes(hours: Long, minutes: Long): String
    fun daysHours(days: Long, hours: Long): String

    /** A runtime under an hour, as the TV and phone screens show it: "58 min". */
    fun runtimeMinutes(n: Long): String

    /** What is left of [time]: "26 min left". */
    fun left(time: String): String

    /** "45 seconds", "12 minutes", "1 hour", "1 hour 12 minutes", "34 minutes left": durations for screen readers. */
    fun spokenSeconds(n: Long): String
    fun spokenMinutes(n: Long): String
    fun spokenHours(n: Long): String
    fun spokenHoursMinutes(hours: String, minutes: String): String
    fun spokenLeft(time: String): String

    /** A time there isn't: "never". */
    val never: String

    /** [unit]'s now: "now" (seconds), "this minute", "today", "this week". */
    fun thisTime(unit: RelativeUnit): String

    /** The day, week, month or year before: "yesterday", "last week"; a smaller unit's "1 minute ago". */
    fun lastTime(unit: RelativeUnit): String

    /** The day, week, month or year after: "tomorrow", "next week"; a smaller unit's "in 1 minute". */
    fun nextTime(unit: RelativeUnit): String

    /** "3 minutes ago". */
    fun ago(n: Long, unit: RelativeUnit): String

    /** "in 5 hours". */
    fun later(n: Long, unit: RelativeUnit): String

    /** "S1 · E3", "S1", "E3": an episode's numbers, short. */
    fun seasonEpisode(season: Int, episode: Int): String
    fun seasonShort(season: Int): String
    fun episodeShort(episode: Int): String

    /** "Season 2", "Episode 5", "Season" without a number, "Specials" (season 0). */
    fun season(number: Int): String
    fun episode(number: Int): String
    val seasonUnnumbered: String
    val specials: String

    /** "3 seasons", "1 episode", "12,345 albums": [n] of [what], written [number]. */
    fun count(what: Counted, n: Int, number: String): String

    /** A tile's state for screen readers: "watched", "6 unwatched". */
    val watched: String
    fun unwatched(n: Int): String

    /** A track with neither a language nor a title: "Unknown". */
    val unknown: String

    /** "Stereo", "5ch": channels without a name of their own. */
    val stereo: String
    fun channels(n: Int): String

    /** A subtitle track's flags: "Forced", "External". */
    val forced: String
    val external: String

    /** "10-bit". */
    fun bitDepth(n: Int): String

    /** A listing's line when neither its bitrate nor its size is known: "Bitrate unknown". */
    val bitrateUnknown: String

    /** A version with nothing to tell it by: "Version 2". */
    fun version(n: Int): String

    /** The dynamic range filter that takes any HDR: "Any HDR". */
    val anyHdr: String

    /** [number] of 1024 to the power [power]: "1.5 KB" (0 bytes, 1 KB, 2 MB, 3 GB, 4 TB). */
    fun size(number: String, power: Int): String

    /** "8.2 Mbps", "320 kbps". */
    fun megabits(number: String): String
    fun kilobits(number: String): String

    /** What the transcoder does: "Video transcode", "Video", "HEVC (copy)", "cuda decode", "burn-in subs". */
    val videoTranscode: String
    val video: String
    fun copied(stream: String): String
    fun decoded(decoder: String): String
    val burnIn: String

    /** "Level 5.1", "23.976 fps": a codec's level, a frame rate. */
    fun level(level: String): String
    fun frameRate(rate: String): String

    /** Dolby Vision in full: "Profile 8.1", "HDR10-compatible base layer", "no compatible base layer", its enhancement layer. */
    fun dvProfile(profile: String): String
    fun dvBaseLayer(format: String): String
    val dvNoBaseLayer: String
    val dvFullLayer: String
    val dvMinimalLayer: String

    /**
     * Pley's own name for a code that names no one language, or that
     * names one Locale calls otherwise: "und", "mul", "zxx", "qaa", "cmn"
     * (Mandarin, which Locale calls Chinese). `null` for any other.
     */
    fun ownLanguageName(code: String): String?

    companion object {
        /** The English the format functions always wrote. */
        val English: FormatWords = EnglishWords
    }
}

/** The units of relative times, as Intl.RelativeTimeFormat has them. */
enum class RelativeUnit { Second, Minute, Hour, Day, Week, Month, Year }

/** What [FormatWords.count] counts. */
enum class Counted { Seasons, Episodes, Albums }

private object EnglishWords : FormatWords {
    override val locale: Locale = Locale.ENGLISH
    override val decimalSeparator: Char = '.'

    override fun seconds(n: Long) = "${n}s"
    override fun minutes(n: Long) = "${n}m"
    override fun hours(n: Long) = "${n}h"
    override fun hoursMinutes(hours: Long, minutes: Long) = "${hours}h ${minutes}m"
    override fun daysHours(days: Long, hours: Long) = "${days}d ${hours}h"
    override fun runtimeMinutes(n: Long) = "$n min"
    override fun left(time: String) = "$time left"

    override fun spokenSeconds(n: Long) = counted(n, "second")
    override fun spokenMinutes(n: Long) = counted(n, "minute")
    override fun spokenHours(n: Long) = counted(n, "hour")
    override fun spokenHoursMinutes(hours: String, minutes: String) = "$hours $minutes"
    override fun spokenLeft(time: String) = "$time left"

    override val never = "never"

    override fun thisTime(unit: RelativeUnit) = when (unit) {
        RelativeUnit.Second -> "now"
        RelativeUnit.Day -> "today"
        else -> "this ${unit.word}"
    }

    override fun lastTime(unit: RelativeUnit) = when {
        unit == RelativeUnit.Day -> "yesterday"
        unit > RelativeUnit.Day -> "last ${unit.word}"
        else -> ago(1, unit)
    }

    override fun nextTime(unit: RelativeUnit) = when {
        unit == RelativeUnit.Day -> "tomorrow"
        unit > RelativeUnit.Day -> "next ${unit.word}"
        else -> later(1, unit)
    }

    override fun ago(n: Long, unit: RelativeUnit) = "${counted(n, unit.word)} ago"

    override fun later(n: Long, unit: RelativeUnit) = "in ${counted(n, unit.word)}"

    override fun seasonEpisode(season: Int, episode: Int) = "S$season · E$episode"
    override fun seasonShort(season: Int) = "S$season"
    override fun episodeShort(episode: Int) = "E$episode"
    override fun season(number: Int) = "Season $number"
    override fun episode(number: Int) = "Episode $number"
    override val seasonUnnumbered = "Season"
    override val specials = "Specials"

    override fun count(what: Counted, n: Int, number: String): String {
        val one = when (what) {
            Counted.Seasons -> "season"
            Counted.Episodes -> "episode"
            Counted.Albums -> "album"
        }
        return "$number ${if (n == 1) one else "${one}s"}"
    }

    override val watched = "watched"
    override fun unwatched(n: Int) = "$n unwatched"

    override val unknown = "Unknown"
    override val stereo = "Stereo"
    override fun channels(n: Int) = "${n}ch"
    override val forced = "Forced"
    override val external = "External"
    override fun bitDepth(n: Int) = "$n-bit"
    override val bitrateUnknown = "Bitrate unknown"
    override fun version(n: Int) = "Version $n"
    override val anyHdr = "Any HDR"

    override fun size(number: String, power: Int) = "$number ${listOf("B", "KB", "MB", "GB", "TB")[power]}"
    override fun megabits(number: String) = "$number Mbps"
    override fun kilobits(number: String) = "$number kbps"

    override val videoTranscode = "Video transcode"
    override val video = "Video"
    override fun copied(stream: String) = "$stream (copy)"
    override fun decoded(decoder: String) = "$decoder decode"
    override val burnIn = "burn-in subs"

    override fun level(level: String) = "Level $level"
    override fun frameRate(rate: String) = "$rate fps"

    override fun dvProfile(profile: String) = "Profile $profile"
    override fun dvBaseLayer(format: String) = "$format-compatible base layer"
    override val dvNoBaseLayer = "no compatible base layer"
    override val dvFullLayer = "full enhancement layer"
    override val dvMinimalLayer = "minimal enhancement layer"

    override fun ownLanguageName(code: String): String? = code.takeIf { it in OWN_LANGUAGE_NAMES }?.let(::englishLanguageName)

    private val RelativeUnit.word: String get() = name.lowercase(Locale.ROOT)

    private fun counted(n: Long, one: String): String = if (n == 1L) "1 $one" else "$n ${one}s"
}
