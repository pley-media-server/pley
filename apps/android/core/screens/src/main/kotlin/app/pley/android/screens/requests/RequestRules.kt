package app.pley.android.screens.requests

import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import app.pley.android.data.summary
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverSeason
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.DownloadState
import app.pley.android.model.MediaRequest
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestAction
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus

/*
 * Requests' rules, as every client keeps them: pure functions of what the
 * server answered. Their words are RequestTexts'.
 */

// ---------------------------------------------------------------- badges

/** A title's badge on its poster. */
enum class TitleBadge { InLibrary, PartlyInLibrary, Requested, Declined }

/** [title]'s badge: none for titles that may be asked for, or can't be. */
fun badgeOf(title: DiscoverTitle): TitleBadge? = badgeOf(title.status, title.itemId)

fun badgeOf(status: TitleStatus, itemId: Long?): TitleBadge? = when (status) {
    TitleStatus.Library -> TitleBadge.InLibrary
    TitleStatus.Partial -> if (itemId != null) TitleBadge.PartlyInLibrary else TitleBadge.Requested
    TitleStatus.Requested -> TitleBadge.Requested
    TitleStatus.Declined -> TitleBadge.Declined
    TitleStatus.Restricted, TitleStatus.None, TitleStatus.Unknown -> null
}

/**
 * [title] as one of its requests now says it is (requests_changed, in rows
 * already shown): active → requested, declined → declined, available → in
 * the library, gone → no badge. A request no longer there (`null`, a 404:
 * no longer followed, withdrawn while others follow it, deleted) says
 * nothing of the title, which others may still ask for or a library have:
 * only the request is let go of.
 */
fun patched(title: DiscoverTitle, request: MediaRequest?): DiscoverTitle = when {
    request == null -> title.copy(requestId = null)
    request.stage == RequestStage.Gone -> title.copy(status = TitleStatus.None, requestId = null)
    request.status == RequestStatus.Available -> title.copy(status = TitleStatus.Library, itemId = request.itemId ?: title.itemId)
    request.status == RequestStatus.Declined -> title.copy(status = TitleStatus.Declined, requestId = null)
    request.active -> title.copy(status = TitleStatus.Requested, requestId = request.id)
    else -> title
}

/** Pending, approved or failed: it still asks for something. */
val MediaRequest.active: Boolean
    get() = status == RequestStatus.Pending || status == RequestStatus.Approved || status == RequestStatus.Failed

/** Something of it downloads or goes into the library: worth reading again every 10 s. */
val MediaRequest.moving: Boolean
    get() = download != null || stage == RequestStage.Downloading || stage == RequestStage.Importing

// ---------------------------------------------------------------- stage pills

/** A pill's colour: pending amber, on its way blue, ready green, refused red, gone gray. */
enum class PillTone { Amber, Blue, Green, Red, Gray }

/** Where a request is, as its pill says it. */
sealed interface StagePill {
    val tone: PillTone

    data object Pending : StagePill {
        override val tone get() = PillTone.Amber
    }

    data object Approved : StagePill {
        override val tone get() = PillTone.Blue
    }

    /** Not out yet: out at [at] (a movie), airing at [at] (a show), or when isn't known. */
    data class Waiting(val show: Boolean, val at: Long?) : StagePill {
        override val tone get() = PillTone.Blue
    }

    data object Searching : StagePill {
        override val tone get() = PillTone.Blue
    }

    /** [progress] 0-1, done at [eta]; [state] says when it waits, is paused or has a problem. */
    data class Downloading(val progress: Float, val eta: Long?, val state: DownloadState) : StagePill {
        override val tone get() = PillTone.Blue
    }

    data object Importing : StagePill {
        override val tone get() = PillTone.Blue
    }

    /** Shows: [seasons] are ready. */
    data class Partial(val seasons: List<Int>) : StagePill {
        override val tone get() = PillTone.Green
    }

    data object Available : StagePill {
        override val tone get() = PillTone.Green
    }

    data object Gone : StagePill {
        override val tone get() = PillTone.Gray
    }

    data object Declined : StagePill {
        override val tone get() = PillTone.Red
    }

    /** Failed, as managers see it: what went wrong, and when the server tries again ([retryAt]), if it does. */
    data class CouldNotSend(val reason: String?, val retryAt: Long?) : StagePill {
        override val tone get() = PillTone.Red
    }

    /** Failed, as everyone else sees it: the server tries again at [retryAt], or leaves it to an administrator. */
    data class NotSentYet(val retryAt: Long?) : StagePill {
        override val tone get() = PillTone.Red
    }

    /** A stage this app doesn't know. */
    data object Unknown : StagePill {
        override val tone get() = PillTone.Gray
    }
}

/** [request]'s pill, as a manager ([manage]) or anyone else sees it. */
fun stagePill(request: MediaRequest, manage: Boolean): StagePill = when (request.stage) {
    RequestStage.Pending -> StagePill.Pending
    RequestStage.Approved -> StagePill.Approved
    RequestStage.Waiting -> StagePill.Waiting(request.kind == TitleKind.Show, request.stageAt)
    RequestStage.Searching -> StagePill.Searching
    RequestStage.Downloading ->
        StagePill.Downloading(downloadProgress(request) ?: 0f, request.download?.eta, request.download?.state ?: DownloadState.Downloading)
    RequestStage.Importing -> StagePill.Importing
    RequestStage.Partial -> StagePill.Partial(request.availableSeasons)
    RequestStage.Available -> StagePill.Available
    RequestStage.Gone -> StagePill.Gone
    RequestStage.Declined -> StagePill.Declined
    RequestStage.Failed -> if (manage) StagePill.CouldNotSend(request.reason, request.retryAt) else StagePill.NotSentYet(request.retryAt)
    RequestStage.Unknown -> StagePill.Unknown
}

/** How far [request]'s download is, 0-1, while there is one: the bar under its pill (a partial one's too). */
fun downloadProgress(request: MediaRequest): Float? = request.download?.progress?.toFloat()?.coerceIn(0f, 1f)

/** Ready (some or all of it) in a library this account can open: Watch opens it there. */
fun readyToWatch(request: MediaRequest): Boolean =
    request.itemId != null && (request.stage == RequestStage.Available || request.stage == RequestStage.Partial)

/** Ready for someone who follows it, but in no library they can open: "ask an administrator" instead of Watch. */
fun readyElsewhere(request: MediaRequest): Boolean =
    request.itemId == null && (request.stage == RequestStage.Available || request.stage == RequestStage.Partial)

// ---------------------------------------------------------------- allowance

/** How much this account may still ask for in the last 7 days, and when more. `null`s: no limit. */
data class Allowance(
    val moviesLeft: Int?,
    val seasonsLeft: Int?,
    val moviesLimit: Int?,
    val seasonsLimit: Int?,
    val moviesNextAt: Long?,
    val seasonsNextAt: Long?,
) {
    /** Some limit applies: the allowance line has something to say. */
    val limited: Boolean get() = moviesLeft != null || seasonsLeft != null

    /** Whether one more of [kind] may be asked for. */
    fun allows(kind: TitleKind): Boolean = when (kind) {
        TitleKind.Movie -> moviesLeft == null || moviesLeft > 0
        TitleKind.Show -> seasonsLeft == null || seasonsLeft > 0
        TitleKind.Unknown -> true
    }

    /** What's left of [kind]: movies, or seasons; `null`: no limit. */
    fun left(kind: TitleKind): Int? = if (kind == TitleKind.Show) seasonsLeft else moviesLeft

    companion object {
        fun of(summary: RequestSummary): Allowance = Allowance(
            summary.moviesLeft, summary.seasonsLeft, summary.moviesLimit, summary.seasonsLimit, summary.moviesNextAt, summary.seasonsNextAt,
        )

        /** A title page's, which the server sends with it; the limits from [summary]. */
        fun of(detail: DiscoverDetail, summary: RequestSummary?): Allowance = Allowance(
            detail.moviesLeft, detail.seasonsLeft, summary?.moviesLimit, summary?.seasonsLimit, detail.moviesNextAt, detail.seasonsNextAt,
        )
    }
}

// ---------------------------------------------------------------- the main action

/** Whether asking works for a title now. */
enum class Ask {
    Yes,

    /** The account may not ask: "Requests aren't open to this account." */
    NoAccess,

    /** Nothing left to ask for: the button is disabled, and the allowance line shows. */
    Limit,
}

/** A title page's one main action, the first that applies. */
sealed interface MainAction {
    /** This sign-in came before requests: only "Sign in again". */
    data object SignInAgain : MainAction

    /** Above the account's age limit: no button, the restriction's text. */
    data class Restricted(val text: String?) : MainAction

    /** In a library: Open, where [itemId] is known. */
    data class Open(val itemId: Long?) : MainAction

    /** No button: "Declined: “{reason}”. You can ask again in 12 days." */
    data class Declined(val reason: String?, val againAt: Long?) : MainAction

    /** [Requested], inert, with [request]'s pill. */
    data class Mine(val request: MediaRequest?) : MainAction

    /** [Following], inert, with [request]'s pill, and Stop following. */
    data class Following(val request: MediaRequest?) : MainAction

    /** Someone asked for it already: Tell me too, which costs nothing. */
    data class TellMeToo(val ask: Ask) : MainAction

    /** Some of it is here or asked for: Open (when [itemId] is set), and Request more seasons unless [more] is `null`. */
    data class Partial(val itemId: Long?, val more: Ask?) : MainAction

    data class Request(val ask: Ask) : MainAction
}

/** Whether asking works for [kind] under [state], with [allowance] left. */
fun askFor(kind: TitleKind, state: RequestsState, allowance: Allowance?): Ask = when {
    !state.mayAsk || state.summary?.access == RequestAccess.Off -> Ask.NoAccess
    allowance != null && !allowance.allows(kind) -> Ask.Limit
    else -> Ask.Yes
}

/** [detail]'s main action, for what [state] lets this sign-in do. */
fun mainAction(detail: DiscoverDetail, state: RequestsState): MainAction {
    if (state is RequestsState.SignInAgain) return MainAction.SignInAgain
    val ask = askFor(detail.kind, state, Allowance.of(detail, state.summary))
    return when (detail.status) {
        TitleStatus.Restricted -> MainAction.Restricted(detail.restriction)
        TitleStatus.Library -> MainAction.Open(detail.itemId)
        TitleStatus.Declined -> MainAction.Declined(detail.declined?.reason, detail.declined?.againAt?.takeIf { it > 0 })
        TitleStatus.Requested -> {
            val active = activeRequest(detail)
            when {
                active?.mine == true -> MainAction.Mine(active)
                detail.watching || active?.watching == true -> MainAction.Following(active)
                // Following costs nothing: only access counts.
                else -> MainAction.TellMeToo(if (ask == Ask.NoAccess) Ask.NoAccess else Ask.Yes)
            }
        }
        TitleStatus.Partial -> MainAction.Partial(detail.itemId, ask.takeIf { moreSeasons(detail) })
        TitleStatus.None, TitleStatus.Unknown -> MainAction.Request(ask)
    }
}

/**
 * A title's main action from what a row knows of it (the hero's, before
 * its page is read): a request it has is taken as this account's.
 */
fun titleAction(title: DiscoverTitle, state: RequestsState, allowance: Allowance?): MainAction {
    if (state is RequestsState.SignInAgain) return MainAction.SignInAgain
    val ask = askFor(title.kind, state, allowance)
    return when (title.status) {
        TitleStatus.Restricted -> MainAction.Restricted(null)
        TitleStatus.Library -> MainAction.Open(title.itemId)
        TitleStatus.Declined -> MainAction.Declined(null, null)
        TitleStatus.Requested -> if (title.requestId != null) MainAction.Mine(null) else MainAction.TellMeToo(if (ask == Ask.NoAccess) Ask.NoAccess else Ask.Yes)
        TitleStatus.Partial -> MainAction.Partial(title.itemId, ask)
        TitleStatus.None, TitleStatus.Unknown -> MainAction.Request(ask)
    }
}

/** The active request [detail] names (its `request_id`), else the newest active one this account sees. */
fun activeRequest(detail: DiscoverDetail): MediaRequest? =
    detail.requests.firstOrNull { it.id == detail.requestId } ?: detail.requests.firstOrNull { it.active }

/** The request [season] shows the stage of: the one asking for it, among the page's. */
fun requestOf(season: DiscoverSeason, detail: DiscoverDetail): MediaRequest? =
    season.requestId?.let { id -> detail.requests.firstOrNull { it.id == id } }

/** Tell me too: the movie, or the seasons others asked for, which costs nothing. */
fun tellMeToo(detail: DiscoverDetail): NewRequest = when (detail.kind) {
    TitleKind.Show -> NewRequest(
        detail.kind,
        detail.tmdbId,
        detail.seasons.orEmpty().filter { it.state == SeasonState.Requested }.map { it.number }.takeIf { it.isNotEmpty() },
    )
    else -> NewRequest(detail.kind, detail.tmdbId)
}

/** Asking again for [request] as it was (Request again). */
fun askAgain(request: MediaRequest): NewRequest =
    NewRequest(request.kind, request.tmdbId, request.seasons?.takeIf { request.kind == TitleKind.Show && it.isNotEmpty() })

/** The longest reason a decline takes (the server's limit): a longer one, pasted say, keeps its first characters. */
const val DECLINE_REASON_MAX = 500

/**
 * Where a decline's [reason] ends once cut to [DECLINE_REASON_MAX]
 * characters: its length when it fits, else after that many, or one
 * before when the last would be the first half of a pair (an emoji), which
 * alone would reach the server as "?".
 */
fun declineReasonEnd(reason: CharSequence): Int = when {
    reason.length <= DECLINE_REASON_MAX -> reason.length
    reason[DECLINE_REASON_MAX - 1].isHighSurrogate() -> DECLINE_REASON_MAX - 1
    else -> DECLINE_REASON_MAX
}

// ---------------------------------------------------------------- seasons

/** Whether [season] may be asked for: missing, partly in a library or not aired, not declined, the title not restricted. */
fun askable(season: DiscoverSeason, detail: DiscoverDetail): Boolean =
    season.state in ASKABLE && !season.declined && detail.status != TitleStatus.Restricted

private val ASKABLE = setOf(SeasonState.Missing, SeasonState.Partial, SeasonState.Unaired)

/** Missing or partly here, and not the specials: what the picker ticks first. */
private fun wanted(season: DiscoverSeason): Boolean =
    season.number != 0 && (season.state == SeasonState.Missing || season.state == SeasonState.Partial)

/** "Request more seasons" shows: a season other than the specials is askable and missing or partly here. */
fun moreSeasons(detail: DiscoverDetail): Boolean = detail.seasons.orEmpty().any { wanted(it) && askable(it, detail) }

/** One season in the picker: tickable when [askable]. */
data class PickerSeason(val season: DiscoverSeason, val askable: Boolean)

/**
 * The season picker ("Which seasons?"): every season, specials last;
 * [picked] the ones ticked. More than [left] ticked disables the button
 * and shows the allowance line. [start] is the season it was opened at
 * (a season's poster, on TVs), where focus starts.
 */
data class SeasonPicker(
    val seasons: List<PickerSeason>,
    val picked: Set<Int>,
    val left: Int?,
    val start: Int? = null,
) {
    val count: Int get() = picked.size

    /** More ticked than may be asked for. */
    val overLimit: Boolean get() = left != null && count > left

    /** Request can be pressed. */
    val ready: Boolean get() = count > 0 && !overLimit

    /** [number] ticked or unticked; seasons that can't be asked for stay as they are. */
    fun toggle(number: Int): SeasonPicker {
        if (seasons.none { it.season.number == number && it.askable }) return this
        return copy(picked = if (number in picked) picked - number else picked + number)
    }

    /** What was picked, lowest first, for the request. */
    fun asked(): List<Int> = picked.sorted()

    companion object {
        /**
         * [detail]'s seasons: those askable and missing or partly here
         * (not the specials) are ticked lowest first, up to [left];
         * unaired seasons and specials are offered unticked. Opened at
         * [start], that season is ticked too when it may be asked for.
         */
        fun of(detail: DiscoverDetail, left: Int?, start: Int? = null): SeasonPicker {
            val all = detail.seasons.orEmpty().sortedWith(compareBy<DiscoverSeason> { it.number == 0 }.thenBy { it.number })
            val rows = all.map { PickerSeason(it, askable(it, detail)) }
            val wanted = rows.filter { it.askable && wanted(it.season) }.map { it.season.number }
            val picked = if (left == null) wanted else wanted.take(left.coerceAtLeast(0))
            val chosen = start?.takeIf { n -> rows.any { it.askable && it.season.number == n } }
            return SeasonPicker(rows, picked.toSet() + listOfNotNull(chosen), left, start)
        }
    }
}

/**
 * Season numbers as ranges: `[1, 2, 3, 5]` → "1–3, 5" (an en dash), in
 * order, each once. The specials (0) aren't among them: they're said apart.
 */
fun seasonRanges(numbers: Collection<Int>): String {
    val sorted = numbers.filter { it > 0 }.distinct().sorted()
    if (sorted.isEmpty()) return ""
    val parts = mutableListOf<String>()
    var start = sorted.first()
    var end = start
    for (n in sorted.drop(1)) {
        if (n == end + 1) {
            end = n
        } else {
            parts += range(start, end)
            start = n
            end = n
        }
    }
    parts += range(start, end)
    return parts.joinToString(", ")
}

private fun range(start: Int, end: Int): String = if (start == end) "$start" else "$start–$end"

// ---------------------------------------------------------------- actions

/**
 * What a request card offers: exactly the server's actions, in its order,
 * those this app doesn't know left out. Where [dialogs] can be shown,
 * Approve comes with Approve… (the dialog for profile, folder and seasons)
 * and Decline asks for an optional reason in its dialog; elsewhere Decline
 * comes with "Decline with a reason…".
 */
enum class CardAction { Withdraw, Remove, StopFollowing, Approve, ApproveWith, TryAgain, Decline, DeclineWithReason, Handled, Delete, AskAgain }

fun cardActions(request: MediaRequest, dialogs: Boolean): List<CardAction> = request.actions.distinct().flatMap { action ->
    when (action) {
        RequestAction.Withdraw -> listOf(if (request.status == RequestStatus.Declined) CardAction.Remove else CardAction.Withdraw)
        RequestAction.Unwatch -> listOf(CardAction.StopFollowing)
        RequestAction.Approve -> if (dialogs) listOf(CardAction.Approve, CardAction.ApproveWith) else listOf(CardAction.Approve)
        RequestAction.Retry -> listOf(CardAction.TryAgain)
        RequestAction.Decline -> if (dialogs) listOf(CardAction.Decline) else listOf(CardAction.Decline, CardAction.DeclineWithReason)
        RequestAction.Handled -> listOf(CardAction.Handled)
        RequestAction.Delete -> listOf(CardAction.Delete)
        RequestAction.AskAgain -> listOf(CardAction.AskAgain)
        RequestAction.Unknown -> emptyList()
    }
}

/** Whether [this] asks first: "Withdraw your request for “X”?", "Remove “X” from your requests?", "Delete this request?". */
val CardAction.confirms: Boolean
    get() = this == CardAction.Withdraw || this == CardAction.Remove || this == CardAction.Delete

/** Whether [this] decides for everyone: a manager's, shown after the person's own. */
val CardAction.managing: Boolean
    get() = this == CardAction.Approve || this == CardAction.ApproveWith || this == CardAction.TryAgain ||
        this == CardAction.Decline || this == CardAction.DeclineWithReason || this == CardAction.Handled || this == CardAction.Delete
