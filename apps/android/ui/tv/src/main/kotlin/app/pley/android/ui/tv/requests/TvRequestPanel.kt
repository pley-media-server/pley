package app.pley.android.ui.tv.requests

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.TextFieldBuffer
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.MediaRequest
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.CardAction
import app.pley.android.screens.requests.DECLINE_REASON_MAX
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.cardActions
import app.pley.android.screens.requests.confirms
import app.pley.android.screens.requests.downloadProgress
import app.pley.android.screens.requests.stagePill
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.TvTextField
import app.pley.android.ui.tv.kit.focusRow
import app.pley.android.ui.tv.playlists.TvPanelRow
import app.pley.android.ui.tv.playlists.TvPanelTitle
import kotlinx.coroutines.launch

/** Where the panel is: the request's actions, a question before one that can't be undone, or a decline's reason. */
@Immutable
private sealed interface Step {
    data object Actions : Step

    data class Confirm(val action: CardAction) : Step

    data object Reason : Step
}

/**
 * What may be done with [request], in a side panel (requests-page.tsx's
 * RequestPanel), opened with OK on its card: at the top its title, stage,
 * download, who asked and when, and its notes; then Open where a library
 * has it for this profile, the server's actions in its order (Decline
 * comes with "Decline with a reason…", Approve is plain: the settings'
 * choices apply), and Details, its title's page. What can't be undone
 * asks first, in the panel, Cancel first; a reason is typed on the TV's
 * keyboard, which opens at once and whose Done goes on to Decline. Back
 * from a question goes back to the actions; from the actions it closes
 * the panel, focus back on the card ([returnTo]; `null` when the card went
 * meanwhile and focus has gone to the one in its place).
 */
@Composable
internal fun TvRequestPanel(
    request: MediaRequest?,
    manage: Boolean,
    busy: Boolean,
    texts: RequestTexts,
    onAction: (MediaRequest, CardAction, String?) -> Unit,
    onOpen: (PleyKey) -> Unit,
    onClose: () -> Unit,
    returnTo: (Long) -> FocusRequester?,
) {
    var last by remember { mutableStateOf(request) }
    if (request != null) last = request
    val shown = last
    var step by remember(shown?.id) { mutableStateOf<Step>(Step.Actions) }
    var used by remember(shown?.id) { mutableStateOf<String?>(null) }
    // Closed, focus goes back to the card once the panel has let go of it (it keeps focus while it is open).
    var wasOpen by remember { mutableStateOf(false) }
    LaunchedEffect(request != null) {
        if (request != null) {
            wasOpen = true
        } else if (wasOpen) {
            wasOpen = false
            withFrameNanos { }
            withFrameNanos { }
            shown?.let { returnTo(it.id) }?.let { runCatching { it.requestFocus() } }
        }
    }
    TvSidePanel(visible = request != null, onDismiss = onClose, modifier = Modifier.testTag(TV_REQUEST_PANEL)) {
        if (shown == null) return@TvSidePanel
        // Back from a question goes back to the actions, not out of the panel.
        BackHandler(enabled = request != null && step != Step.Actions) { step = Step.Actions }
        val list = rememberLazyListState()
        val first = remember(step) { FocusRequester() }
        LaunchedEffect(step, request != null) {
            // The reason's field takes focus itself, its keyboard open.
            if (request != null && step != Step.Reason) {
                withFrameNanos { }
                withFrameNanos { }
                list.focusRow(first)
            }
        }
        val act: (CardAction, String?) -> Unit = { action, reason -> if (!busy) onAction(shown, action, reason) }
        when (val now = step) {
            Step.Actions -> Head(shown, manage, texts)
            is Step.Confirm -> TvPanelTitle(texts.confirm(now.action, shown.title).orEmpty())
            Step.Reason -> {
                TvPanelTitle(stringResource(ScreensR.string.requests_decline_title, shown.title))
                if (!shown.mine) {
                    BasicText(
                        stringResource(ScreensR.string.requests_decline_reason_hint),
                        style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                        modifier = Modifier.padding(start = 16.dp, end = 8.dp, bottom = 12.dp),
                    )
                }
            }
        }
        LazyColumn(
            state = list,
            modifier = Modifier
                .fillMaxSize()
                // Focus stays in the panel until Back closes it.
                .focusProperties { onExit = { if (request != null) cancelFocusChange() } }
                .focusGroup(),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            when (val now = step) {
                Step.Actions -> {
                    val actions = cardActions(shown, dialogs = false)
                    val open = shown.itemId
                    val start = used ?: when {
                        open != null -> OPEN
                        actions.isNotEmpty() -> actions.first().name
                        else -> DETAILS
                    }
                    if (open != null) {
                        item(key = OPEN) {
                            TvPanelRow(
                                label = stringResource(ScreensR.string.requests_open),
                                icon = PleyIcons.Play,
                                onClick = { onOpen(PleyKey.Item(open)) },
                                focusRequester = if (start == OPEN) first else null,
                                modifier = Modifier.testTag(TV_REQUEST_ROW + OPEN),
                            )
                        }
                    }
                    actions.forEach { action ->
                        item(key = action.name) {
                            TvPanelRow(
                                label = texts.action(action),
                                icon = iconOf(action),
                                danger = action == CardAction.Delete,
                                onClick = {
                                    used = action.name
                                    when {
                                        action == CardAction.DeclineWithReason -> step = Step.Reason
                                        action.confirms -> step = Step.Confirm(action)
                                        else -> act(action, null)
                                    }
                                },
                                focusRequester = if (start == action.name) first else null,
                                modifier = Modifier.testTag(TV_REQUEST_ROW + action.name),
                            )
                        }
                    }
                    item(key = DETAILS) {
                        TvPanelRow(
                            label = stringResource(ScreensR.string.requests_details),
                            icon = PleyIcons.Info,
                            onClick = { onOpen(PleyKey.DiscoverTitle(shown.kind, shown.tmdbId)) },
                            focusRequester = if (start == DETAILS) first else null,
                            modifier = Modifier.testTag(TV_REQUEST_ROW + DETAILS),
                        )
                    }
                }
                is Step.Confirm -> {
                    // Cancel first, as the app's other questions: OK pressed twice undoes nothing.
                    item(key = CANCEL) {
                        TvPanelRow(stringResource(ScreensR.string.requests_cancel), onClick = { step = Step.Actions }, icon = PleyIcons.ArrowLeft, focusRequester = first)
                    }
                    item(key = "confirm") {
                        TvPanelRow(
                            texts.action(now.action),
                            onClick = { act(now.action, null) },
                            icon = iconOf(now.action),
                            danger = true,
                            modifier = Modifier.testTag(TV_REQUEST_ROW + "confirm"),
                        )
                    }
                }
                Step.Reason -> {
                    item(key = "reason") { ReasonField(first, onDone = { act(CardAction.DeclineWithReason, it) }, onCancel = { step = Step.Actions }) }
                }
            }
        }
    }
}

/** The request at the top of its panel: title (year), stage, download, what was asked and by whom, its notes. */
@Composable
private fun Head(request: MediaRequest, manage: Boolean, texts: RequestTexts) {
    val pill = stagePill(request, manage)
    TvPanelTitle(request.year?.let { "${request.title} ($it)" } ?: request.title)
    Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        TvStagePill(pill, texts, small = true)
        request.download?.let { download ->
            Spacer(Modifier.height(8.dp))
            TvDownloadBar(downloadProgress(request) ?: 0f, download.state, Modifier.width(288.dp))
        }
        Spacer(Modifier.height(6.dp))
        val what = request.seasons?.takeIf { request.kind == TitleKind.Show && it.isNotEmpty() }?.let(texts::seasons) ?: texts.kind(request.kind)
        Caption("$what · ${texts.asked(request, manage)}", lines = 2)
        texts.pillLine(pill)?.let { Caption(it, lines = 3) }
        request.note?.takeIf { manage && it.isNotBlank() }?.let { Caption(it, lines = 2) }
        texts.sentTo(request)?.takeIf { manage }?.let { Caption(it, lines = 1) }
    }
}

@Composable
private fun Caption(text: String, lines: Int) {
    BasicText(text, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = lines, overflow = TextOverflow.Ellipsis)
}

/**
 * Declining with a reason: the field, whose keyboard opens at once (its
 * Done goes on to Decline), then Decline and Cancel.
 */
@Composable
private fun ReasonField(first: FocusRequester, onDone: (String) -> Unit, onCancel: () -> Unit) {
    val field = rememberTextFieldState()
    val decline = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Box(Modifier.padding(start = 16.dp, end = 8.dp, bottom = 12.dp)) {
            TvTextField(
                state = field,
                placeholder = stringResource(ScreensR.string.requests_decline_reason),
                imeAction = ImeAction.Done,
                // Once the field has taken focus back from the keyboard's Done, the words are in: Decline is next.
                onAction = {
                    scope.launch {
                        withFrameNanos { }
                        runCatching { decline.requestFocus() }
                    }
                },
                focusModifier = Modifier.focusRequester(first),
                typeNow = true,
                inputTransformation = ReasonLength,
                modifier = Modifier.width(288.dp),
            )
        }
        TvPanelRow(
            stringResource(ScreensR.string.requests_decline),
            onClick = { onDone(field.text.toString()) },
            icon = PleyIcons.TicketX,
            focusRequester = decline,
            modifier = Modifier.testTag(TV_REQUEST_ROW + "decline"),
        )
        TvPanelRow(stringResource(ScreensR.string.requests_cancel), onClick = onCancel, icon = PleyIcons.ArrowLeft)
    }
}

/** A reason past [DECLINE_REASON_MAX] characters (pasted, or a remote app's) keeps its first ones. */
private object ReasonLength : InputTransformation {
    override fun TextFieldBuffer.transformInput() {
        if (length > DECLINE_REASON_MAX) replace(DECLINE_REASON_MAX, length, "")
    }
}

private fun iconOf(action: CardAction): PleyIcon = when (action) {
    CardAction.Withdraw, CardAction.Remove -> PleyIcons.X
    CardAction.StopFollowing -> PleyIcons.BookmarkMinus
    CardAction.Approve, CardAction.ApproveWith -> PleyIcons.Check
    CardAction.TryAgain -> PleyIcons.RotateCcw
    CardAction.Decline, CardAction.DeclineWithReason -> PleyIcons.TicketX
    CardAction.Handled -> PleyIcons.CircleCheck
    CardAction.Delete -> PleyIcons.Delete
    CardAction.AskAgain -> PleyIcons.Plus
}

private const val OPEN = "open"
private const val DETAILS = "details"
private const val CANCEL = "cancel"

internal const val TV_REQUEST_PANEL = "pley.tv.requests.panel"
internal const val TV_REQUEST_ROW = "pley.tv.requests.panel."
