package app.pley.android.ui.phone.requests

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.TitleKind
import app.pley.android.screens.R
import app.pley.android.screens.requests.Allowance
import app.pley.android.screens.requests.Ask
import app.pley.android.screens.requests.MainAction
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneShell

/** What a title's main action does, for [TitleActions]. */
internal class TitleActionHandlers(
    /** Request: a movie is asked for; a show's picker opens. */
    val request: () -> Unit,
    /** Request more seasons: the picker. */
    val more: () -> Unit,
    val tellMeToo: () -> Unit,
    val stopFollowing: () -> Unit,
    /** Open: the library's title. */
    val open: (Long) -> Unit,
)

/**
 * A title's one main action, the first that applies:
 * Sign in; a restriction's words; Open; why it was declined;
 * Requested or Following (inert) with the request's stage and Stop
 * following; Tell me too; Open and Request more seasons; Request. With
 * nothing left to ask for, Request is disabled and the allowance says
 * when more may be asked for; without access, a line says so. [busy]
 * holds the buttons while an ask is on its way.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun TitleActions(
    action: MainAction,
    kind: TitleKind,
    allowance: Allowance?,
    manage: Boolean,
    busy: Boolean,
    texts: RequestTexts,
    on: TitleActionHandlers,
    modifier: Modifier = Modifier,
    /** The allowance shows under Request even with some left (the title page; the hero says it only at the limit). */
    allowanceAlways: Boolean = true,
) {
    val shell = LocalPhoneShell.current
    Column(modifier.testTag(TITLE_ACTION), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        when (action) {
            MainAction.SignInAgain -> PrimaryButton(stringResource(R.string.requests_sign_in), onClick = shell::signInAgain)
            is MainAction.Restricted -> action.text?.takeIf { it.isNotBlank() }?.let { Line(it, PleyIcons.Lock) }
            is MainAction.Open -> {
                val itemId = action.itemId
                if (itemId != null) {
                    PrimaryButton(stringResource(R.string.requests_open), onClick = { on.open(itemId) }, icon = PleyIcons.Play)
                } else {
                    Line(stringResource(R.string.requests_ready_elsewhere), PleyIcons.Info)
                }
            }
            is MainAction.Declined -> Line(texts.declined(action), PleyIcons.TicketX)
            is MainAction.Mine -> {
                Inert(stringResource(R.string.requests_requested))
                action.request?.let { RequestStage(it, manage, texts) }
            }
            is MainAction.Following -> {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                    Inert(stringResource(R.string.requests_following))
                    PleyTextButton(stringResource(R.string.requests_stop_following), onClick = on.stopFollowing, enabled = !busy)
                }
                action.request?.let { RequestStage(it, manage, texts) }
            }
            is MainAction.TellMeToo -> if (action.ask == Ask.NoAccess) {
                NoAccess()
            } else {
                PrimaryButton(stringResource(R.string.requests_tell_me_too), onClick = on.tellMeToo, icon = PleyIcons.BookmarkPlus, enabled = !busy)
                Line(stringResource(R.string.requests_asked_already))
            }
            is MainAction.Partial -> {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    action.itemId?.let { itemId ->
                        PrimaryButton(stringResource(R.string.requests_open), onClick = { on.open(itemId) }, icon = PleyIcons.Play)
                    }
                    if (action.more == Ask.Yes || action.more == Ask.Limit) {
                        TonalButton(
                            stringResource(R.string.requests_more_seasons),
                            onClick = on.more,
                            icon = PleyIcons.Plus,
                            enabled = action.more == Ask.Yes && !busy,
                        )
                    }
                }
                when (action.more) {
                    Ask.NoAccess -> NoAccess()
                    Ask.Limit -> AllowanceLines(allowance, TitleKind.Show, texts)
                    Ask.Yes -> if (allowanceAlways) AllowanceLines(allowance, TitleKind.Show, texts)
                    null -> Unit
                }
            }
            is MainAction.Request -> when (action.ask) {
                Ask.NoAccess -> NoAccess()
                else -> {
                    PrimaryButton(
                        stringResource(R.string.requests_request),
                        onClick = on.request,
                        icon = PleyIcons.Plus,
                        enabled = action.ask == Ask.Yes && !busy,
                    )
                    if (action.ask == Ask.Limit || allowanceAlways) AllowanceLines(allowance, kind, texts)
                }
            }
        }
    }
}

/** The person's own request or one they follow: the button that says so, doing nothing. */
@Composable
private fun Inert(label: String) {
    TonalButton(label, onClick = {}, icon = PleyIcons.Check, enabled = false)
}

/** "Requests aren't open to this account." */
@Composable
private fun NoAccess() {
    Line(stringResource(R.string.requests_no_access_line), PleyIcons.Lock)
}

/** The allowance for [kind]: what is left and the limits, or when more may be asked for. */
@Composable
internal fun AllowanceLines(allowance: Allowance?, kind: TitleKind?, texts: RequestTexts, modifier: Modifier = Modifier) {
    val lines = allowance?.let { texts.allowance(it, kind) }.orEmpty()
    if (lines.isEmpty()) return
    Column(modifier) {
        lines.forEach { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted) }
    }
}

/** A line of the action's: why there is no button, or what one means. */
@Composable
private fun Line(text: String, icon: PleyIcon? = null) {
    Row(verticalAlignment = Alignment.Top) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(8.dp))
        }
        Text(text, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
    }
}

internal const val TITLE_ACTION = "pley.requests.action"
