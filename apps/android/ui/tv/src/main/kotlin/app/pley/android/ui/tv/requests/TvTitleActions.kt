package app.pley.android.ui.tv.requests

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestAction
import app.pley.android.model.TitleKind
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.Allowance
import app.pley.android.screens.requests.Ask
import app.pley.android.screens.requests.MainAction
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.stagePill
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton

/** What a title's main action does, for [TvTitleActions]. */
@Stable
internal class TvTitleHandlers(
    /** Request: a movie is asked for; a show's picker opens. */
    val request: () -> Unit,
    /** Request more seasons: the picker. */
    val more: () -> Unit,
    val tellMeToo: () -> Unit,
    val stopFollowing: () -> Unit,
    /** Open: the library's title. */
    val open: (Long) -> Unit,
    /** Sign in again: its dialog. */
    val signInAgain: () -> Unit,
)

/** The main action's pill: one slot, so focus stays on it as Request turns into Requested. */
@Immutable
private class MainPill(val label: String, val icon: PleyIcon, val enabled: Boolean, val onClick: () -> Unit)

/** A line of what the action says: why there is no button, what one means, the limit. */
@Immutable
private class Note(val text: String, val icon: PleyIcon? = null, val danger: Boolean = false)

/**
 * A title's one main action as a row of buttons (packages/tv's
 * TitleActions), the first that applies: Sign in again; a restriction's
 * words; Open; why it was declined; Requested or Following, which say
 * where things are and do nothing (they keep focus, as every button
 * does), with the request's stage and download beside them and Stop
 * following; Tell me too; Open and Request more seasons; Request. At the
 * limit Request is disabled and the allowance says when more may be
 * asked for; without access, a line says so.
 *
 * What it says comes under the buttons, the stage's own line first; over
 * them where no button of the main action's shows, and in [compact] (the
 * hero of Discover) only its first line, over them. [mainModifier] goes
 * on the main pill while it can take focus (its requester, the page's
 * initial focus); [extra] adds the page's own buttons after, told
 * whether the main action took the initial focus.
 */
@Composable
internal fun TvTitleActions(
    action: MainAction,
    kind: TitleKind,
    allowance: Allowance?,
    manage: Boolean,
    texts: RequestTexts,
    on: TvTitleHandlers,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
    mainModifier: Modifier = Modifier,
    extra: @Composable RowScope.(claimed: Boolean) -> Unit = {},
) {
    val notes = ArrayList<Note>(3)
    var main: MainPill? = null
    var more: MainPill? = null
    var stage: MediaRequest? = null
    var unwatch = false
    val noAccess = Note(stringResource(ScreensR.string.requests_no_access_line), PleyIcons.Lock)
    fun allowanceFor(of: TitleKind) {
        allowance?.let { texts.allowance(it, of) }?.forEach { notes += Note(it) }
    }
    when (action) {
        MainAction.SignInAgain -> {
            main = MainPill(stringResource(ScreensR.string.requests_sign_in), PleyIcons.LogOut, true, on.signInAgain)
            notes += Note(stringResource(ScreensR.string.requests_sign_in_body))
        }
        is MainAction.Restricted -> action.text?.takeIf { it.isNotBlank() }?.let { notes += Note(it, PleyIcons.Lock) }
        is MainAction.Open -> {
            val itemId = action.itemId
            main = MainPill(stringResource(ScreensR.string.requests_open), PleyIcons.Play, itemId != null) { if (itemId != null) on.open(itemId) }
            if (itemId == null) notes += Note(stringResource(ScreensR.string.requests_ready_elsewhere), PleyIcons.Info)
        }
        is MainAction.Declined -> notes += Note(texts.declined(action), PleyIcons.TicketX, danger = true)
        is MainAction.Mine -> {
            main = MainPill(stringResource(ScreensR.string.requests_requested), PleyIcons.Check, true) {}
            stage = action.request
        }
        is MainAction.Following -> {
            main = MainPill(stringResource(ScreensR.string.requests_following), PleyIcons.Check, true) {}
            stage = action.request
            unwatch = action.request?.actions?.contains(RequestAction.Unwatch) == true
        }
        is MainAction.TellMeToo -> if (action.ask == Ask.NoAccess) {
            notes += noAccess
        } else {
            main = MainPill(stringResource(ScreensR.string.requests_tell_me_too), PleyIcons.BookmarkPlus, true, on.tellMeToo)
            notes += Note(stringResource(ScreensR.string.requests_asked_already))
        }
        is MainAction.Partial -> {
            val itemId = action.itemId
            val asking = action.more == Ask.Yes || action.more == Ask.Limit
            val request = MainPill(stringResource(ScreensR.string.requests_more_seasons), PleyIcons.Plus, action.more == Ask.Yes, on.more)
            if (itemId != null) {
                main = MainPill(stringResource(ScreensR.string.requests_open), PleyIcons.Play, true) { on.open(itemId) }
                if (asking) more = request
            } else if (asking) {
                main = request
            }
            when (action.more) {
                Ask.NoAccess -> notes += noAccess
                Ask.Limit -> allowanceFor(TitleKind.Show)
                Ask.Yes -> if (!compact) allowanceFor(TitleKind.Show)
                null -> Unit
            }
        }
        is MainAction.Request -> if (action.ask == Ask.NoAccess) {
            notes += noAccess
        } else {
            main = MainPill(stringResource(ScreensR.string.requests_request), PleyIcons.Plus, action.ask == Ask.Yes, on.request)
            if (action.ask == Ask.Limit || !compact) allowanceFor(kind)
        }
    }
    // The stage's own line comes first: it says more of the pill beside the button.
    stage?.let { texts.pillLine(stagePill(it, manage)) }?.let { notes.add(0, Note(it)) }
    val said = if (compact) notes.take(1) else notes
    val above = compact || main == null
    val claimed = main?.enabled == true
    Column(modifier.testTag(TV_TITLE_ACTIONS)) {
        if (above && said.isNotEmpty()) {
            Notes(said)
            Spacer(Modifier.height(14.dp))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap), verticalAlignment = Alignment.Top) {
            main?.let { pill ->
                TvPillButton(
                    label = pill.label,
                    onClick = pill.onClick,
                    icon = pill.icon,
                    enabled = pill.enabled,
                    modifier = if (pill.enabled) mainModifier else Modifier,
                )
            }
            more?.let { pill -> TvPillButton(pill.label, onClick = pill.onClick, icon = pill.icon, enabled = pill.enabled) }
            stage?.let { request ->
                // Beside a 40 dp button, centred on it.
                Box(Modifier.height(PleyTv.ButtonHeight), contentAlignment = Alignment.CenterStart) {
                    TvStageBlock(request, manage, texts, Modifier.widthIn(max = StageWidth))
                }
            }
            if (unwatch) TvRoundButton(stringResource(ScreensR.string.requests_stop_following), PleyIcons.BookmarkMinus, onClick = on.stopFollowing)
            extra(claimed)
        }
        if (!above && said.isNotEmpty()) {
            Spacer(Modifier.height(12.dp))
            Notes(said)
        }
    }
}

/** What the action says, muted and close together: a stage's line, the allowance, why it can't be asked for. */
@Composable
private fun Notes(notes: List<Note>) {
    Column(Modifier.widthIn(max = NotesWidth), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        notes.forEach { note ->
            Row(verticalAlignment = Alignment.Top) {
                if (note.icon != null) {
                    Icon(note.icon, contentDescription = null, size = 16.dp, tint = if (note.danger) PleyColors.DangerText else PleyColors.Muted, modifier = Modifier.padding(top = 2.dp))
                    Spacer(Modifier.width(8.dp))
                }
                BasicText(note.text, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 2)
            }
        }
    }
}

/** The widest a stage gets beside a button. */
private val StageWidth = 260.dp

/** The text column: x 96 to 560. */
private val NotesWidth = 464.dp

internal const val TV_TITLE_ACTIONS = "pley.tv.requests.action"
