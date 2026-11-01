package app.pley.android.screens.requests

import android.content.res.Resources
import app.pley.android.common.sentenceOf
import app.pley.android.data.RequestNotice
import app.pley.android.model.Asked
import app.pley.android.model.DiscoverList
import app.pley.android.model.DiscoverSeason
import app.pley.android.model.DownloadState
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestScope
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import app.pley.android.model.format.formatDate
import app.pley.android.model.format.formatRelative
import app.pley.android.screens.R
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.temporal.ChronoUnit
import java.util.Locale
import kotlin.math.ceil
import kotlin.math.roundToInt

/**
 * Requests' words from [res]: what the rules decide, said. Times are
 * relative to [now] (unix seconds) and days are in [zone]: "in 3 days",
 * "tomorrow", "Out Nov 4".
 */
class RequestTexts(
    private val res: Resources,
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    private val locale: Locale = Locale.getDefault(),
    private val zone: ZoneId = ZoneId.systemDefault(),
) {
    // ---------------------------------------------------------------- titles

    fun badge(badge: TitleBadge): String = res.getString(
        when (badge) {
            TitleBadge.InLibrary -> R.string.requests_badge_library
            TitleBadge.PartlyInLibrary -> R.string.requests_badge_partial
            TitleBadge.Requested -> R.string.requests_badge_requested
            TitleBadge.Declined -> R.string.requests_badge_declined
        },
    )

    fun kind(kind: TitleKind): String = res.getString(if (kind == TitleKind.Show) R.string.requests_kind_show else R.string.requests_kind_movie)

    fun row(list: DiscoverList): String = res.getString(
        when (list) {
            DiscoverList.Trending -> R.string.requests_row_trending
            DiscoverList.Movies -> R.string.requests_row_movies
            DiscoverList.Shows -> R.string.requests_row_shows
            DiscoverList.Upcoming -> R.string.requests_row_upcoming
            DiscoverList.Anime -> R.string.requests_row_anime
        },
    )

    /** "Declined: “Not for us”. You can ask again in 12 days." */
    fun declined(action: MainAction.Declined): String {
        val reason = action.reason?.trim()?.takeIf { it.isNotEmpty() }
        val again = action.againAt?.let(::until)
        return when {
            reason != null && again != null -> res.getString(R.string.requests_declined_reason, reason, again)
            again != null -> res.getString(R.string.requests_declined_plain, again)
            reason != null -> res.getString(R.string.requests_declined_reason_only, reason)
            else -> res.getString(R.string.requests_declined_only)
        }
    }

    // ---------------------------------------------------------------- seasons

    /** "Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials". */
    fun seasons(numbers: Collection<Int>): String {
        val specials = 0 in numbers
        val rest = numbers.filter { it > 0 }.distinct()
        val ranges = seasonRanges(rest)
        return when {
            rest.isEmpty() -> if (specials) res.getString(R.string.requests_specials) else ""
            rest.size == 1 -> res.getString(if (specials) R.string.requests_season_and_specials else R.string.requests_season, ranges)
            else -> res.getString(if (specials) R.string.requests_seasons_and_specials else R.string.requests_seasons, ranges)
        }
    }

    /** A season's name on its own: TMDB's, else "Season 2" or "Specials". */
    fun seasonName(season: DiscoverSeason): String = season.name.takeIf { it.isNotBlank() } ?: seasons(listOf(season.number))

    /** How a season stands for this account: "In library", "4 of 10 episodes in library", "Airs Nov 4, 2026", "10 episodes". */
    fun seasonState(season: DiscoverSeason): String = when {
        season.declined -> res.getString(R.string.requests_season_declined)
        season.state == SeasonState.Library -> res.getString(R.string.requests_season_library)
        season.state == SeasonState.Requested -> res.getString(R.string.requests_season_requested)
        season.state == SeasonState.Partial -> res.getString(R.string.requests_season_partial, season.inLibrary, maxOf(season.episodes, season.inLibrary))
        season.state == SeasonState.Unaired ->
            season.airDate?.let { res.getString(R.string.requests_season_airs, formatDate(it, locale)) } ?: res.getString(R.string.requests_season_unaired)
        else -> res.getQuantityString(R.plurals.requests_season_episodes, season.episodes, season.episodes)
    }

    // ---------------------------------------------------------------- stages

    /** The words of [pill]: "Waiting for approval", "Downloading 42% · in 20 min", "Out Nov 4". */
    fun pill(pill: StagePill): String = when (pill) {
        StagePill.Pending -> res.getString(R.string.requests_stage_pending)
        StagePill.Approved -> res.getString(R.string.requests_stage_approved)
        is StagePill.Waiting -> pill.at?.let { at ->
            res.getString(if (pill.show) R.string.requests_stage_airs else R.string.requests_stage_out, day(at))
        } ?: res.getString(R.string.requests_stage_not_out)
        StagePill.Searching -> res.getString(R.string.requests_stage_searching)
        is StagePill.Downloading -> when (pill.state) {
            DownloadState.Queued -> res.getString(R.string.requests_download_queued)
            DownloadState.Paused -> res.getString(R.string.requests_download_paused)
            DownloadState.Warning -> res.getString(R.string.requests_download_warning)
            else -> {
                val words = res.getString(R.string.requests_stage_downloading, (pill.progress * 100).toInt())
                val eta = pill.eta?.takeIf { it > now() }
                if (eta != null) res.getString(R.string.requests_stage_eta, words, until(eta)) else words
            }
        }
        StagePill.Importing -> res.getString(R.string.requests_stage_importing)
        is StagePill.Partial -> res.getString(R.string.requests_stage_partial, seasons(pill.seasons))
        StagePill.Available -> res.getString(R.string.requests_stage_available)
        StagePill.Gone -> res.getString(R.string.requests_stage_gone)
        StagePill.Declined -> res.getString(R.string.requests_stage_declined)
        is StagePill.CouldNotSend ->
            pill.reason?.takeIf { it.isNotBlank() }?.let { res.getString(R.string.requests_stage_failed, it) }
                ?: res.getString(R.string.requests_stage_failed_plain)
        is StagePill.NotSentYet -> res.getString(R.string.requests_stage_not_sent)
        StagePill.Unknown -> res.getString(R.string.requests_stage_unknown)
    }

    /** The line under a failed request's pill: when the server tries again, or that an administrator will look. */
    fun pillLine(pill: StagePill): String? = when (pill) {
        is StagePill.NotSentYet ->
            pill.retryAt?.let { res.getString(R.string.requests_trying_again, until(it)) } ?: res.getString(R.string.requests_not_sent_body)
        is StagePill.CouldNotSend -> pill.retryAt?.let { res.getString(R.string.requests_trying_again, until(it)) }
        else -> null
    }

    /** Who asked and when: "Ann · 3 days ago", "Asked for by someone else · 3 days ago", or just when. */
    fun asked(request: MediaRequest, manage: Boolean): String {
        val at = ago(request.createdAt)
        val by = request.requestedBy?.name
        return when {
            request.mine && !manage -> at
            by != null -> res.getString(R.string.requests_by_when, by, at)
            else -> res.getString(R.string.requests_by_when, res.getString(R.string.requests_by_someone), at)
        }
    }

    fun watchers(count: Int): String = res.getQuantityString(R.plurals.requests_watchers, count, count)

    /** Managers: where it went, "Radarr · HD-1080p · /movies"; `null` before it was sent. */
    fun sentTo(request: MediaRequest): String? {
        val sent = request.sentTo ?: return null
        val service = res.getString(if (request.kind == TitleKind.Show) R.string.requests_sent_to_sonarr else R.string.requests_sent_to_radarr)
        return listOfNotNull(service, sent.qualityProfile, sent.rootFolder).joinToString(" · ")
    }

    // ---------------------------------------------------------------- allowance

    /**
     * The allowance line, for [kind] (`null`: both): "You can ask for 2
     * more movies and 5 more seasons." with "Limits: 3 movies and 10
     * seasons per 7 days.", or when more can be asked for. Empty without
     * limits.
     */
    fun allowance(allowance: Allowance, kind: TitleKind? = null): List<String> {
        val movies = kind != TitleKind.Show && allowance.moviesLeft != null
        val shows = kind != TitleKind.Movie && allowance.seasonsLeft != null
        if (!movies && !shows) return emptyList()
        val moviesOut = movies && allowance.moviesLeft == 0
        val seasonsOut = shows && allowance.seasonsLeft == 0
        val moviesAt = allowance.moviesNextAt?.let(::until)
        val seasonsAt = allowance.seasonsNextAt?.let(::until)
        if (moviesOut && seasonsOut) {
            return listOf(
                if (moviesAt != null && seasonsAt != null) res.getString(R.string.requests_all_both, moviesAt, seasonsAt)
                else res.getString(R.string.requests_all_for_now),
            )
        }
        if (moviesOut && !shows) return listOf(moviesAt?.let { res.getString(R.string.requests_all_movies, it) } ?: res.getString(R.string.requests_all_for_now))
        if (seasonsOut && !movies) return listOf(seasonsAt?.let { res.getString(R.string.requests_all_seasons, it) } ?: res.getString(R.string.requests_all_for_now))
        val lines = mutableListOf<String>()
        if (moviesOut) lines += moviesAt?.let { res.getString(R.string.requests_all_movies, it) } ?: res.getString(R.string.requests_all_for_now)
        if (seasonsOut) lines += seasonsAt?.let { res.getString(R.string.requests_all_seasons, it) } ?: res.getString(R.string.requests_all_for_now)
        val left = listOfNotNull(
            allowance.moviesLeft?.takeIf { movies && it > 0 }?.let { res.getQuantityString(R.plurals.requests_more_movies, it, it) },
            allowance.seasonsLeft?.takeIf { shows && it > 0 }?.let { res.getQuantityString(R.plurals.requests_more_seasons_count, it, it) },
        )
        when (left.size) {
            2 -> lines += res.getString(R.string.requests_allowance_both, left[0], left[1])
            1 -> lines += res.getString(R.string.requests_allowance_one, left[0])
        }
        val limits = listOfNotNull(
            allowance.moviesLimit?.takeIf { movies }?.let { res.getQuantityString(R.plurals.requests_movies, it, it) },
            allowance.seasonsLimit?.takeIf { shows }?.let { res.getQuantityString(R.plurals.requests_seasons, it, it) },
        )
        when (limits.size) {
            2 -> lines += res.getString(R.string.requests_limits_both, limits[0], limits[1])
            1 -> lines += res.getString(R.string.requests_limits_one, limits[0])
        }
        return lines
    }

    // ---------------------------------------------------------------- asking

    /** What asking for [title] did: asked (waiting, approved, or not sent yet), or followed. */
    fun asked(asked: Asked, title: String): String = res.getString(
        when {
            !asked.created -> R.string.requests_asked_following
            asked.request.status == RequestStatus.Approved || asked.request.status == RequestStatus.Available -> R.string.requests_asked_approved
            asked.request.status == RequestStatus.Failed -> R.string.requests_asked_failed
            else -> R.string.requests_asked_pending
        },
        title,
    )

    /** "Request 3 seasons", or "Pick a season." with none picked. */
    fun pickerButton(picker: SeasonPicker): String =
        if (picker.count == 0) res.getString(R.string.requests_picker_none) else res.getQuantityString(R.plurals.requests_picker_request, picker.count, picker.count)

    // ---------------------------------------------------------------- actions

    fun action(action: CardAction): String = res.getString(
        when (action) {
            CardAction.Withdraw -> R.string.requests_withdraw
            CardAction.Remove -> R.string.requests_remove
            CardAction.StopFollowing -> R.string.requests_stop_following
            CardAction.Approve -> R.string.requests_approve
            CardAction.ApproveWith -> R.string.requests_approve_with
            CardAction.TryAgain -> R.string.requests_try_again
            CardAction.Decline -> R.string.requests_decline
            CardAction.DeclineWithReason -> R.string.requests_decline_with_reason
            CardAction.Handled -> R.string.requests_handled
            CardAction.Delete -> R.string.requests_delete
            CardAction.AskAgain -> R.string.requests_ask_again
        },
    )

    /** The question [action] asks first, for [title]; `null` when it asks none. */
    fun confirm(action: CardAction, title: String): String? = when (action) {
        CardAction.Withdraw -> res.getString(R.string.requests_withdraw_confirm, title)
        CardAction.Remove -> res.getString(R.string.requests_remove_confirm, title)
        CardAction.Delete -> res.getString(R.string.requests_delete_confirm)
        else -> null
    }

    // ---------------------------------------------------------------- notices

    /**
     * The number on the way into requests ([entry]'s badge), said for
     * screen readers: "3 requests need you, 1 couldn't be sent" for
     * managers, "2 of your requests have news" for others.
     */
    fun count(entry: RequestsEntry): String {
        if (!entry.manage) return res.getQuantityString(R.plurals.requests_notice_news, entry.badge, entry.badge)
        val need = res.getQuantityString(R.plurals.requests_count_manage, entry.badge, entry.badge)
        if (entry.failed <= 0) return need
        return res.getString(R.string.requests_count_and, need, res.getQuantityString(R.plurals.requests_count_failed, entry.failed, entry.failed))
    }

    fun notice(notice: RequestNotice): String = when (notice) {
        is RequestNotice.Ready -> {
            val seasons = notice.seasons.orEmpty()
            when {
                notice.complete || seasons.isEmpty() -> res.getString(R.string.requests_notice_ready, notice.title)
                seasons.size == 1 -> res.getString(R.string.requests_notice_season_ready, seasons(seasons), notice.title)
                else -> res.getString(R.string.requests_notice_seasons_ready, seasons(seasons), notice.title)
            }
        }
        is RequestNotice.Approved -> res.getString(R.string.requests_notice_approved, notice.title)
        is RequestNotice.Declined -> notice.reason?.trim()?.takeIf { it.isNotEmpty() }
            ?.let { res.getString(R.string.requests_notice_declined_reason, notice.title, it) }
            ?: res.getString(R.string.requests_notice_declined, notice.title)
        is RequestNotice.Asked -> {
            val by = notice.by?.takeIf { it.isNotBlank() } ?: res.getString(R.string.requests_notice_someone)
            val seasons = notice.seasons?.takeIf { it.isNotEmpty() }
            if (seasons != null) res.getString(R.string.requests_notice_asked_seasons, by, notice.title, seasons(seasons))
            else res.getString(R.string.requests_notice_asked, by, notice.title)
        }
        is RequestNotice.Failed -> {
            val show = notice.kind == TitleKind.Show
            notice.reason?.trim()?.takeIf { it.isNotEmpty() }?.let {
                res.getString(if (show) R.string.requests_notice_failed_sonarr else R.string.requests_notice_failed_radarr, notice.title, it)
            } ?: res.getString(if (show) R.string.requests_notice_failed_sonarr_plain else R.string.requests_notice_failed_radarr_plain, notice.title)
        }
        is RequestNotice.News -> res.getQuantityString(R.plurals.requests_notice_news, notice.count, notice.count)
    }

    // ---------------------------------------------------------------- problems

    /** What [problem] says, its first line; [summary] for the limits and whether this sign-in manages. */
    fun problem(problem: RequestProblem, summary: RequestSummary?): String = when (problem) {
        RequestProblem.Off -> res.getString(R.string.requests_off_title)
        RequestProblem.NoAccess -> res.getString(R.string.requests_no_access_title)
        is RequestProblem.SignInAgain ->
            res.getString(if (problem.scope == RequestScope.Approvals) R.string.requests_sign_in_approvals else R.string.requests_sign_in_title)
        RequestProblem.TmdbKey -> res.getString(R.string.requests_tmdb_key)
        RequestProblem.TmdbDown -> res.getString(R.string.requests_tmdb_down)
        is RequestProblem.Restricted -> problem.text
        is RequestProblem.Declined -> problem.retryAt?.let { res.getString(R.string.requests_ask_again_at, until(it)) }
            ?: res.getString(R.string.requests_declined_only)
        is RequestProblem.Limit -> problem.retryAt?.let { res.getString(R.string.requests_ask_more_at, until(it)) } ?: atMost(summary)
        is RequestProblem.Conflict -> problem.message
        RequestProblem.NotFound -> res.getString(R.string.requests_not_found)
        is RequestProblem.Offline -> sentenceOf(problem.error)
        is RequestProblem.Other -> sentenceOf(problem.error)
    }

    /** The line under [problem]'s, where it has one. */
    fun problemBody(problem: RequestProblem, summary: RequestSummary?): String? = when (problem) {
        RequestProblem.Off -> res.getString(R.string.requests_off_body)
        RequestProblem.NoAccess -> res.getString(R.string.requests_no_access_body)
        is RequestProblem.SignInAgain -> if (problem.scope == RequestScope.Approvals) null else res.getString(R.string.requests_sign_in_body)
        RequestProblem.TmdbKey -> res.getString(if (summary?.manage == true) R.string.requests_tmdb_key_manage else R.string.requests_tmdb_key_others)
        RequestProblem.TmdbDown -> res.getString(R.string.requests_tmdb_down_body)
        RequestProblem.NotFound -> res.getString(R.string.requests_not_found_body)
        else -> null
    }

    /** 429 with no time: more was asked than the limit allows at all. */
    private fun atMost(summary: RequestSummary?): String {
        val seasons = summary?.seasonsLimit
        val movies = summary?.moviesLimit
        return when {
            seasons != null -> res.getString(R.string.requests_at_most_seasons, res.getQuantityString(R.plurals.requests_seasons, seasons, seasons))
            movies != null -> res.getString(R.string.requests_at_most_movies, res.getQuantityString(R.plurals.requests_movies, movies, movies))
            else -> res.getString(R.string.requests_all_for_now)
        }
    }

    // ---------------------------------------------------------------- times

    /** How long until [ts]: "in 5 min", "in 3 hours", "tomorrow", "in 12 days"; "in a moment" once it's past. */
    fun until(ts: Long): String {
        val diff = ts - now()
        if (diff <= 0) return res.getString(R.string.requests_soon)
        if (diff < HOUR) {
            val minutes = ceil(diff / 60.0).toInt().coerceAtLeast(1)
            return res.getQuantityString(R.plurals.requests_in_minutes, minutes, minutes)
        }
        if (diff < DAY) {
            val hours = (diff / HOUR.toDouble()).roundToInt().coerceIn(1, 23)
            return res.getQuantityString(R.plurals.requests_in_hours, hours, hours)
        }
        val days = ChronoUnit.DAYS.between(localDate(now()), localDate(ts)).toInt().coerceAtLeast(1)
        return if (days == 1) res.getString(R.string.requests_tomorrow) else res.getQuantityString(R.plurals.requests_in_days, days, days)
    }

    /** How long ago [ts] was: "3 days ago", "yesterday". */
    fun ago(ts: Long): String = formatRelative(ts, now())

    /** A day, "Nov 4" ("Nov 4, 2027" in another year), with the time within 24 hours: "today, 9:00 PM". */
    fun day(ts: Long): String {
        val at = Instant.ofEpochSecond(ts).atZone(zone)
        val diff = ts - now()
        if (diff in 0 until DAY) {
            val time = at.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale))
            val today = at.toLocalDate() == localDate(now())
            return res.getString(if (today) R.string.requests_today_at else R.string.requests_tomorrow_at, time)
        }
        val pattern = if (at.year == localDate(now()).year) "MMM d" else "MMM d, yyyy"
        return at.format(DateTimeFormatter.ofPattern(pattern, locale))
    }

    private fun localDate(ts: Long) = Instant.ofEpochSecond(ts).atZone(zone).toLocalDate()

    private companion object {
        const val HOUR = 3_600L
        const val DAY = 86_400L
    }
}
