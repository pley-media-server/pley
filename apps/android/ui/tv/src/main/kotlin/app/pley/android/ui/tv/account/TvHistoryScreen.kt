package app.pley.android.ui.tv.account

import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.account.HistoryRow
import app.pley.android.screens.account.HistoryUiState
import app.pley.android.screens.account.HistoryViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.settings.absolute
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * Watch history on TV: the profile's plays, newest first, one row each
 * with its picture, title, what it is, when and on which device. Select
 * opens it; the cross at the row's end (Right) removes it at once, as
 * does a long press of Select, and focus goes to the play below it (or
 * above, for the last). Show more loads the next 15. Focus starts on the
 * first play.
 */
@Composable
internal fun TvHistoryScreen() {
    val model: HistoryViewModel = viewModel(factory = HistoryViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    var problem by remember { mutableStateOf<UiText?>(null) }
    LaunchedEffect(model) { model.errors.collect { problem = errorSentence(it) } }
    val memory = rememberFocusMemory()
    // Each shown row's focus and its cross's, for the neighbour of a row removed.
    val focus = remember { HashMap<Long, Pair<FocusRequester, FocusRequester>>() }
    Column(Modifier.fillMaxSize().padding(top = TvSafeArea.Vertical).testTag(TV_HISTORY_PAGE)) {
        Row(Modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal), verticalAlignment = Alignment.Bottom) {
            BasicText(stringResource(R.string.tv_account_history), style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg))
            Spacer(Modifier.width(16.dp))
            (state as? HistoryUiState.Ready)?.let {
                BasicText(pluralStringResource(R.plurals.tv_account_history_count, it.total, it.total), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), modifier = Modifier.padding(bottom = 6.dp))
            }
            Spacer(Modifier.weight(1f))
            problem?.let { BasicText(it.asString(), style = PleyType.Tv.Label.copy(color = PleyColors.DangerText), maxLines = 1) }
        }
        Spacer(Modifier.height(20.dp))
        when (val s = state) {
            HistoryUiState.Loading -> Column(Modifier.padding(start = PleyTv.Gutter), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                repeat(5) { Box(Modifier.size(560.dp, 52.dp).background(PleyColors.Surface2, PleyShape.Field)) }
            }
            is HistoryUiState.Failed -> Message(
                title = failedTitle(s.error, stringResource(R.string.tv_account_history_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_account_retry),
                onAction = model::retry,
            )
            is HistoryUiState.Ready -> if (s.rows.isEmpty()) {
                Message(
                    title = stringResource(R.string.tv_account_history_empty_title),
                    body = stringResource(R.string.tv_account_history_empty),
                    action = stringResource(R.string.tv_account_back),
                    onAction = navigator::back,
                )
            } else {
                LazyColumn(
                    Modifier.fillMaxSize().heldKeyStaysIn(HeldEnds.UpDown).focusGroup(),
                    contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, bottom = 120.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    items(s.rows, key = { it.entryId }, contentType = { "row" }) { row ->
                        val requesters = remember { FocusRequester() to FocusRequester() }
                        DisposableEffect(row.entryId) {
                            focus[row.entryId] = requesters
                            onDispose { focus.remove(row.entryId) }
                        }
                        HistoryLine(
                            row,
                            s.origin,
                            requesters,
                            onOpen = { navigator.open(row.opens) },
                            onRemove = { fromCross ->
                                // Focus moves on before the row goes, to the same place in the next row, else the one above.
                                val at = s.rows.indexOfFirst { it.entryId == row.entryId }
                                listOfNotNull(s.rows.getOrNull(at + 1), s.rows.getOrNull(at - 1)).firstOrNull { near ->
                                    val (line, cross) = focus[near.entryId] ?: return@firstOrNull false
                                    runCatching { (if (fromCross) cross else line).requestFocus() }.isSuccess
                                }
                                model.remove(row.entryId)
                            },
                            modifier = Modifier.rememberedFocus(memory, "entry-${row.entryId}", initial = row == s.rows.first()),
                        )
                    }
                    if (s.canLoadMore) {
                        item(key = "more", contentType = "more") {
                            Box(Modifier.padding(top = 12.dp)) {
                                TvPillButton(
                                    stringResource(if (s.loadingMore) R.string.tv_account_history_loading else R.string.tv_account_history_more),
                                    onClick = model::loadMore,
                                    enabled = !s.loadingMore,
                                    icon = PleyIcons.ChevronDown,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * A play: the row opens it; the cross at its end removes it ([onRemove]
 * says whether from the cross). Neither scales: they are rows of a list.
 * [focus] is the row's and the cross's.
 */
@Composable
private fun HistoryLine(
    row: HistoryRow,
    origin: String,
    focus: Pair<FocusRequester, FocusRequester>,
    onOpen: () -> Unit,
    onRemove: (fromCross: Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val art = remember(row.artPaths, origin) { ArtworkImage(row.artPaths.map { absolute(it, origin) }) }
    val rowFocus = remember { MutableInteractionSource() }
    val focused by rowFocus.collectIsFocusedAsState()
    Row(verticalAlignment = Alignment.CenterVertically) {
        Row(
            modifier
                .focusRequester(focus.first)
                .width(ROW_WIDTH)
                .heightIn(min = 64.dp)
                .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Field)
                .combinedClickable(interactionSource = rowFocus, indication = null, role = Role.Button, onLongClick = { onRemove(false) }, onClick = onOpen)
                .padding(horizontal = 12.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Square art and posters share one 48 dp column, so the titles line up.
            Box(Modifier.width(48.dp), contentAlignment = Alignment.Center) {
                Box(
                    Modifier
                        .width(if (row.square) 48.dp else 34.dp)
                        .height(if (row.square) 48.dp else 51.dp)
                        .clip(PleyShape.Badge)
                        .background(PleyColors.Surface2),
                ) {
                    Artwork(art, row.title, Modifier.matchParentSize())
                }
            }
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                BasicText(
                    row.title,
                    style = PleyType.Tv.Body.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Fg),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                BasicText(
                    listOf(row.subtitle, row.time, row.device.orEmpty()).filter { it.isNotEmpty() }.joinToString(" · "),
                    style = PleyType.Tv.Caption.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        RemoveButton(row.title, focus.second) { onRemove(true) }
    }
}

@Composable
private fun RemoveButton(title: String, focus: FocusRequester, onRemove: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val label = stringResource(R.string.tv_account_history_remove, title)
    Box(
        Modifier
            .focusRequester(focus)
            .size(40.dp)
            .clip(PleyShape.Circle)
            .background(if (focused) PleyColors.Accent else PleyColors.Surface3)
            .combinedClickable(interactionSource = interactions, indication = null, role = Role.Button, onClickLabel = label, onClick = onRemove),
        contentAlignment = Alignment.Center,
    ) {
        Icon(PleyIcons.X, contentDescription = label, size = 20.dp, tint = if (focused) PleyColors.OnAccent else PleyColors.Muted)
    }
}

/** A page-sized message with one focused action (empty, failed). */
@Composable
internal fun Message(title: String, body: String, action: String, onAction: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(start = PleyTv.Gutter, top = 60.dp, end = TvSafeArea.Horizontal), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg))
        BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
        Spacer(Modifier.height(12.dp))
        TvPillButton(action, onClick = onAction, modifier = Modifier.initialFocus())
    }
}

private val ROW_WIDTH = 700.dp

internal const val TV_HISTORY_PAGE = "pley.tv.history"
