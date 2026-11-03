package app.pley.android.ui.tv.item

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkCandidates
import app.pley.android.screens.item.ItemAction
import app.pley.android.screens.item.ItemActionsUiState
import app.pley.android.screens.item.ItemActionsViewModel
import app.pley.android.screens.item.ItemEvent
import app.pley.android.screens.item.destination
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import app.pley.android.ui.tv.theme.LocalTvTier

/**
 * An item's actions as a screen of their own (PleyKey.ItemActions), for
 * what opens them by key: its backdrop, its name and the actions as a
 * list with focus on the first (D-pad rule 6). Tiles show the same
 * actions in a side panel over their page instead ([TvTileActionsPanel],
 * the item page's [TvPanels]).
 */
@Composable
internal fun TvItemActionsScreen(itemId: Long, continueWatching: Boolean = false) {
    val model: ItemActionsViewModel = viewModel(factory = ItemActionsViewModel.factory(itemId, continueWatching))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val toasts = remember { TvToastState() }
    val resources = LocalResources.current
    val tier = LocalTvTier.current
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                ItemEvent.NothingToPlay -> toasts.show(resources.getString(R.string.tv_item_nothing_to_play))
                is ItemEvent.Failed -> toasts.report(event.error)
            }
        }
    }
    Box(Modifier.fillMaxSize().testTag(TV_ITEM_ACTIONS)) {
        when (val s = state) {
            ItemActionsUiState.Loading -> Unit
            is ItemActionsUiState.Failed -> TvMessage(
                title = failedTitle(s.error, stringResource(R.string.tv_item_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_item_back),
                onAction = navigator::back,
            )
            is ItemActionsUiState.Ready -> {
                TvBackdrop(remember(s.item, s.origin, tier) { ArtworkCandidates.backdrop(s.item, s.origin, tier) }, s.title, scrim = BackdropScrim.Item)
                Column(Modifier.padding(start = PleyTv.Gutter, top = 58.dp).width(TextColumn)) {
                    BasicText(s.title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
                    if (s.subtitle.isNotEmpty()) BasicText(s.subtitle, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 1)
                    Spacer(Modifier.height(24.dp))
                    s.actions.forEachIndexed { i, action ->
                        PanelRow(
                            label = label(action),
                            icon = if (action == ItemAction.Play || action == ItemAction.Resume) PleyIcons.Play else null,
                            modifier = if (i == 0) Modifier.initialFocus() else Modifier,
                            onClick = {
                                val handled = model.perform(action)
                                val next = if (handled) null else destination(action, s.item)
                                navigator.back()
                                next?.let(navigator::open)
                            },
                        )
                    }
                }
            }
        }
        TvToastHost(toasts)
    }
}

@Composable
private fun label(action: ItemAction): String = stringResource(
    when (action) {
        ItemAction.Play -> R.string.tv_item_action_play
        ItemAction.Resume -> R.string.tv_item_action_resume
        ItemAction.FromStart -> R.string.tv_item_action_from_start
        ItemAction.MarkWatched -> R.string.tv_item_mark_watched
        ItemAction.MarkUnwatched -> R.string.tv_item_mark_unwatched
        ItemAction.RemoveFromContinueWatching -> R.string.tv_item_remove_from_continue_watching
        ItemAction.AddToMyList -> R.string.tv_item_add_to_my_list
        ItemAction.RemoveFromMyList -> R.string.tv_item_remove_from_my_list
        ItemAction.AddToPlaylist -> R.string.tv_item_add_to_playlist
        ItemAction.GoToShow -> R.string.tv_item_action_go_to_show
        ItemAction.GoToSeason -> R.string.tv_item_action_go_to_season
        ItemAction.Open -> R.string.tv_item_action_open
    },
)

internal const val TV_ITEM_ACTIONS = "pley.tv.item.actions"
