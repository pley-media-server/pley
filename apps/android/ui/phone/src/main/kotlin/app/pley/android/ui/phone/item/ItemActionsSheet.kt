package app.pley.android.ui.phone.item

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.item.ItemAction
import app.pley.android.screens.item.ItemActionsUiState
import app.pley.android.screens.item.ItemActionsViewModel
import app.pley.android.screens.item.ItemEvent
import app.pley.android.screens.item.destination
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.SheetAction
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.LocalPlayOn
import app.pley.android.ui.phone.remote.canPlayOn
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell

/**
 * An item's action sheet (PleyKey.ItemActions; design spec §3.4): its
 * name, then Play or Resume, From start, Play on… (while another device
 * takes titles), the watched mark, Remove from Continue Watching (from
 * that row), My List, Add to playlist, and Go to show or season. Choosing
 * one closes the sheet; the action carries on after it.
 */
@Composable
internal fun ItemActionsSheet(itemId: Long, continueWatching: Boolean = false) {
    val model: ItemActionsViewModel = viewModel(factory = ItemActionsViewModel.factory(itemId, continueWatching))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                ItemEvent.NothingToPlay -> shell.toasts.show(resources.getString(R.string.item_nothing_to_play))
                is ItemEvent.Failed -> shell.report(event.error)
            }
        }
    }
    Column(Modifier.fillMaxWidth().testTag(ITEM_ACTIONS)) {
        when (val s = state) {
            ItemActionsUiState.Loading -> Box(Modifier.fillMaxWidth().height(160.dp), contentAlignment = Alignment.Center) {
                DelayedSpinner(stringResource(R.string.item_loading))
            }
            is ItemActionsUiState.Failed -> Text(
                errorSentence(s.error).asString(),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.DangerText,
                modifier = Modifier.padding(24.dp),
            )
            is ItemActionsUiState.Ready -> {
                Text(
                    s.title,
                    style = MaterialTheme.typography.titleMedium,
                    color = PleyColors.Fg,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 4.dp),
                )
                if (s.subtitle.isNotEmpty()) {
                    Text(
                        s.subtitle,
                        style = MaterialTheme.typography.bodyMedium,
                        color = PleyColors.Muted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(horizontal = 24.dp),
                    )
                }
                Box(Modifier.height(8.dp))
                val row: @Composable (ItemAction) -> Unit = { action ->
                    val (label, icon) = actionLook(action)
                    SheetAction(
                        label = label,
                        icon = icon,
                        onClick = {
                            val handled = model.perform(action)
                            val next = if (handled) null else destination(action, s.item)
                            navigator.back()
                            next?.let(navigator::open)
                        },
                    )
                }
                // Play on… after the ways to play it here (a track's, an album's: opening its page).
                val playOn = LocalPlayOn.current
                val playing = s.actions.takeWhile { it in PLAY_ACTIONS }
                playing.forEach { row(it) }
                CastTarget.of(s.item)?.takeIf { canPlayOn(it.kind) }?.let { cast ->
                    SheetAction(
                        label = stringResource(R.string.play_on),
                        icon = PleyIcons.Cast,
                        onClick = {
                            navigator.back()
                            playOn?.playOn(cast)
                        },
                        modifier = Modifier.testTag(ITEM_ACTIONS_PLAY_ON),
                    )
                }
                s.actions.drop(playing.size).forEach { row(it) }
            }
        }
    }
}

/** An action's words and icon. */
@Composable
private fun actionLook(action: ItemAction): Pair<String, PleyIcon> = when (action) {
    ItemAction.Play -> stringResource(R.string.item_action_play) to PleyIcons.Play
    ItemAction.Resume -> stringResource(R.string.item_action_resume) to PleyIcons.Play
    ItemAction.FromStart -> stringResource(R.string.item_action_from_start) to PleyIcons.RotateCcw
    ItemAction.MarkWatched -> stringResource(R.string.item_action_mark_watched) to PleyIcons.Eye
    ItemAction.MarkUnwatched -> stringResource(R.string.item_action_mark_unwatched) to PleyIcons.EyeOff
    ItemAction.RemoveFromContinueWatching -> stringResource(R.string.item_action_remove_from_continue_watching) to PleyIcons.X
    ItemAction.AddToMyList -> stringResource(R.string.item_action_add_to_my_list) to PleyIcons.BookmarkPlus
    ItemAction.RemoveFromMyList -> stringResource(R.string.item_action_remove_from_my_list) to PleyIcons.BookmarkMinus
    ItemAction.AddToPlaylist -> stringResource(R.string.item_action_add_to_playlist) to PleyIcons.ListPlus
    ItemAction.GoToShow -> stringResource(R.string.item_action_go_to_show) to PleyIcons.Tv
    ItemAction.GoToSeason -> stringResource(R.string.item_action_go_to_season) to PleyIcons.Layers
    ItemAction.Open -> stringResource(R.string.item_action_open) to PleyIcons.ArrowRight
}

/** What plays the item here, which Play on… follows. */
private val PLAY_ACTIONS = setOf(ItemAction.Play, ItemAction.Resume, ItemAction.FromStart, ItemAction.Open)

internal const val ITEM_ACTIONS = "pley.item.actions"
internal const val ITEM_ACTIONS_PLAY_ON = "pley.item.actions.play_on"
