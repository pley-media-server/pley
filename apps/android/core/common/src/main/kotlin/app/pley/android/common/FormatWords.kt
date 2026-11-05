package app.pley.android.common

import android.content.res.Resources
import androidx.annotation.PluralsRes
import app.pley.android.model.format.Counted
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.RelativeUnit
import java.text.DecimalFormatSymbols
import java.util.Locale

/**
 * The words of :core:model's format functions in the language of
 * [resources] (strings_format.xml), its numbers in their locale: a
 * screen's resources (`formatWords()` in Compose), or
 * AppLanguage.resources where nothing is drawn. Made again when the
 * language changes, as resources are; the English is FormatWords.English's.
 */
fun formatWords(resources: Resources): FormatWords = ResourceFormatWords(resources)

private class ResourceFormatWords(private val r: Resources) : FormatWords {
    override val locale: Locale = r.configuration.locales[0]
    override val decimalSeparator: Char = DecimalFormatSymbols.getInstance(locale).decimalSeparator

    override fun seconds(n: Long) = r.getString(R.string.format_seconds, n)
    override fun minutes(n: Long) = r.getString(R.string.format_minutes, n)
    override fun hours(n: Long) = r.getString(R.string.format_hours, n)
    override fun hoursMinutes(hours: Long, minutes: Long) = r.getString(R.string.format_hours_minutes, hours, minutes)
    override fun daysHours(days: Long, hours: Long) = r.getString(R.string.format_days_hours, days, hours)
    override fun runtimeMinutes(n: Long) = r.getString(R.string.format_runtime_minutes, n)
    override fun left(time: String) = r.getString(R.string.format_left, time)

    override fun spokenSeconds(n: Long) = plural(R.plurals.format_spoken_seconds, n)
    override fun spokenMinutes(n: Long) = plural(R.plurals.format_spoken_minutes, n)
    override fun spokenHours(n: Long) = plural(R.plurals.format_spoken_hours, n)
    override fun spokenHoursMinutes(hours: String, minutes: String) = r.getString(R.string.format_spoken_hours_minutes, hours, minutes)
    override fun spokenLeft(time: String) = r.getString(R.string.format_spoken_left, time)

    override val never: String get() = r.getString(R.string.format_never)

    override fun thisTime(unit: RelativeUnit) = r.getString(
        when (unit) {
            RelativeUnit.Second -> R.string.format_now
            RelativeUnit.Minute -> R.string.format_this_minute
            RelativeUnit.Hour -> R.string.format_this_hour
            RelativeUnit.Day -> R.string.format_today
            RelativeUnit.Week -> R.string.format_this_week
            RelativeUnit.Month -> R.string.format_this_month
            RelativeUnit.Year -> R.string.format_this_year
        },
    )

    override fun lastTime(unit: RelativeUnit) = when (unit) {
        RelativeUnit.Day -> r.getString(R.string.format_yesterday)
        RelativeUnit.Week -> r.getString(R.string.format_last_week)
        RelativeUnit.Month -> r.getString(R.string.format_last_month)
        RelativeUnit.Year -> r.getString(R.string.format_last_year)
        else -> ago(1, unit)
    }

    override fun nextTime(unit: RelativeUnit) = when (unit) {
        RelativeUnit.Day -> r.getString(R.string.format_tomorrow)
        RelativeUnit.Week -> r.getString(R.string.format_next_week)
        RelativeUnit.Month -> r.getString(R.string.format_next_month)
        RelativeUnit.Year -> r.getString(R.string.format_next_year)
        else -> later(1, unit)
    }

    override fun ago(n: Long, unit: RelativeUnit) = plural(
        when (unit) {
            RelativeUnit.Second -> R.plurals.format_seconds_ago
            RelativeUnit.Minute -> R.plurals.format_minutes_ago
            RelativeUnit.Hour -> R.plurals.format_hours_ago
            RelativeUnit.Day -> R.plurals.format_days_ago
            RelativeUnit.Week -> R.plurals.format_weeks_ago
            RelativeUnit.Month -> R.plurals.format_months_ago
            RelativeUnit.Year -> R.plurals.format_years_ago
        },
        n,
    )

    override fun later(n: Long, unit: RelativeUnit) = plural(
        when (unit) {
            RelativeUnit.Second -> R.plurals.format_in_seconds
            RelativeUnit.Minute -> R.plurals.format_in_minutes
            RelativeUnit.Hour -> R.plurals.format_in_hours
            RelativeUnit.Day -> R.plurals.format_in_days
            RelativeUnit.Week -> R.plurals.format_in_weeks
            RelativeUnit.Month -> R.plurals.format_in_months
            RelativeUnit.Year -> R.plurals.format_in_years
        },
        n,
    )

    override fun seasonEpisode(season: Int, episode: Int) = r.getString(R.string.format_season_episode, season, episode)
    override fun seasonShort(season: Int) = r.getString(R.string.format_season_short, season)
    override fun episodeShort(episode: Int) = r.getString(R.string.format_episode_short, episode)
    override fun season(number: Int) = r.getString(R.string.format_season, number)
    override fun episode(number: Int) = r.getString(R.string.format_episode, number)
    override val seasonUnnumbered: String get() = r.getString(R.string.format_season_unnumbered)
    override val specials: String get() = r.getString(R.string.format_specials)

    override fun count(what: Counted, n: Int, number: String) = r.getQuantityString(
        when (what) {
            Counted.Seasons -> R.plurals.format_seasons
            Counted.Episodes -> R.plurals.format_episodes
            Counted.Albums -> R.plurals.format_albums
        },
        n,
        number,
    )

    override val watched: String get() = r.getString(R.string.format_watched)
    override fun unwatched(n: Int) = r.getQuantityString(R.plurals.format_unwatched, n, n)

    override val unknown: String get() = r.getString(R.string.format_unknown)
    override val stereo: String get() = r.getString(R.string.format_stereo)
    override fun channels(n: Int) = r.getString(R.string.format_channels, n)
    override val forced: String get() = r.getString(R.string.format_forced)
    override val external: String get() = r.getString(R.string.format_external)
    override fun bitDepth(n: Int) = r.getString(R.string.format_bit_depth, n)
    override val bitrateUnknown: String get() = r.getString(R.string.format_bitrate_unknown)
    override fun version(n: Int) = r.getString(R.string.format_version, n)
    override val anyHdr: String get() = r.getString(R.string.format_any_hdr)

    override fun size(number: String, power: Int) = r.getString(
        when (power) {
            0 -> R.string.format_bytes
            1 -> R.string.format_kilobytes
            2 -> R.string.format_megabytes
            3 -> R.string.format_gigabytes
            else -> R.string.format_terabytes
        },
        number,
    )

    override fun megabits(number: String) = r.getString(R.string.format_megabits, number)
    override fun kilobits(number: String) = r.getString(R.string.format_kilobits, number)

    override val videoTranscode: String get() = r.getString(R.string.format_video_transcode)
    override val video: String get() = r.getString(R.string.format_video)
    override fun copied(stream: String) = r.getString(R.string.format_copied, stream)
    override fun decoded(decoder: String) = r.getString(R.string.format_decoded, decoder)
    override val burnIn: String get() = r.getString(R.string.format_burn_in)

    override fun level(level: String) = r.getString(R.string.format_level, level)
    override fun frameRate(rate: String) = r.getString(R.string.format_frame_rate, rate)

    override fun dvProfile(profile: String) = r.getString(R.string.format_dv_profile, profile)
    override fun dvBaseLayer(format: String) = r.getString(R.string.format_dv_base_layer, format)
    override val dvNoBaseLayer: String get() = r.getString(R.string.format_dv_no_base_layer)
    override val dvFullLayer: String get() = r.getString(R.string.format_dv_full_layer)
    override val dvMinimalLayer: String get() = r.getString(R.string.format_dv_minimal_layer)

    override fun ownLanguageName(code: String): String? = when (code) {
        "und" -> r.getString(R.string.format_language_und)
        "mul" -> r.getString(R.string.format_language_mul)
        "zxx" -> r.getString(R.string.format_language_zxx)
        "qaa" -> r.getString(R.string.format_language_qaa)
        "cmn" -> r.getString(R.string.format_language_cmn)
        else -> null
    }

    /** [n] of a count, as [id] says it; a count beyond an Int (none of these) takes the largest's words. */
    private fun plural(@PluralsRes id: Int, n: Long): String =
        r.getQuantityString(id, n.coerceAtMost(Int.MAX_VALUE.toLong()).toInt(), n)
}
