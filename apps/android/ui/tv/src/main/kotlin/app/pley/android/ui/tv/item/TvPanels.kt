package app.pley.android.ui.tv.item

import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.ItemKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemAction
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.destination
import app.pley.android.screens.item.itemActions
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.focusRow

/**
 * The page's side panels (design spec §2.7): Audio and subtitles, More,
 * and a tile's actions. One shows at a time; focus starts on its current
 * choice and stays inside it; Back closes it and gives focus back to the
 * button or row that opened it ([onClose]).
 */
@Composable
internal fun TvPanels(
    panel: Panel?,
    page: ItemPage,
    actions: TvPageActions,
    onClose: (FocusRequester?) -> Unit,
    buttons: HeroButtons,
    onSummary: () -> Unit,
) {
    var last by remember { mutableStateOf(panel) }
    if (panel != null) last = panel
    val shown = last
    val returnTo = when (shown) {
        Panel.Tracks -> buttons.tracks
        Panel.More -> buttons.more
        is Panel.Actions -> shown.returnTo
        null -> null
    }
    TvSidePanel(visible = panel != null, onDismiss = { onClose(returnTo) }, modifier = Modifier.testTag(TV_PANEL)) {
        val first = remember(shown) { FocusRequester() }
        val list = rememberLazyListState()
        LaunchedEffect(shown, panel != null) {
            if (panel != null) {
                withFrameNanos { }
                withFrameNanos { }
                list.focusRow(first)
            }
        }
        LazyColumn(
            state = list,
            modifier = Modifier
                .fillMaxSize()
                // Focus stays in the panel until Back closes it; closing, it may leave
                // (back to the button that opened it) while the panel slides away.
                .focusProperties { onExit = { if (panel != null) cancelFocusChange() } }
                .focusGroup(),
        ) {
            when (shown) {
                Panel.Tracks -> tracks(page, actions, first)
                Panel.More -> more(page, actions, first, buttons.summaryCut, onSummary) { onClose(returnTo) }
                is Panel.Actions -> itemActionsList(shown, actions, first) { onClose(returnTo) }
                null -> Unit
            }
        }
    }
}

/** Version, Audio and Subtitles, each choice ticked; choosing keeps the panel open. */
private fun LazyListScope.tracks(page: ItemPage, actions: TvPageActions, first: FocusRequester) {
    val tracks = page.tracks ?: return
    var firstGiven = false
    fun take(checked: Boolean): FocusRequester? = if (!firstGiven && checked) first.also { firstGiven = true } else null
    item(key = "title", contentType = PANEL_TITLE) { PanelTitle(stringResource(R.string.tv_item_tracks)) }
    if (tracks.versions.isNotEmpty()) {
        item(key = "versions", contentType = PANEL_HEADING) { PanelHeading(stringResource(R.string.tv_item_version)) }
        tracks.versions.forEach { v ->
            val requester = take(v.selected)
            item(key = "version-${v.id}", contentType = PANEL_ROW) {
                PanelRow(v.label, onClick = { actions.model.chooseVersion(v.id) }, checked = v.selected, focusRequester = requester)
            }
        }
    }
    if (tracks.audio.isNotEmpty()) {
        item(key = "audio", contentType = PANEL_HEADING) { PanelHeading(stringResource(R.string.tv_item_audio)) }
        // Automatic: no pick, each device plays its best track of the language.
        val automatic = take(tracks.automaticAudio)
        item(key = "audio-automatic", contentType = PANEL_ROW) {
            val language = tracks.automaticLanguage
            PanelRow(
                if (language != null) stringResource(R.string.tv_item_audio_automatic_language, language) else stringResource(R.string.tv_item_audio_automatic),
                onClick = { actions.model.chooseAudio(null) },
                checked = tracks.automaticAudio,
                focusRequester = automatic,
            )
        }
        tracks.audio.forEach { a ->
            val requester = take(a.selected)
            item(key = "audio-${a.id}", contentType = PANEL_ROW) {
                PanelRow(a.label, onClick = { actions.model.chooseAudio(a.id.toInt()) }, checked = a.selected, focusRequester = requester)
            }
        }
    }
    if (tracks.subtitles.isNotEmpty()) {
        item(key = "subtitles", contentType = PANEL_HEADING) { PanelHeading(stringResource(R.string.tv_item_subtitles)) }
        val off = take(tracks.subtitleIndex == null)
        item(key = "subtitle-off", contentType = PANEL_ROW) {
            PanelRow(stringResource(R.string.tv_item_subtitles_off), onClick = { actions.model.chooseSubtitle(null) }, checked = tracks.subtitleIndex == null, focusRequester = off)
        }
        tracks.subtitles.forEach { s ->
            val requester = take(s.selected)
            item(key = "subtitle-${s.id}", contentType = PANEL_ROW) {
                PanelRow(s.label, onClick = { actions.model.chooseSubtitle(s.id.toInt()) }, checked = s.selected, focusRequester = requester)
            }
        }
    }
}

/** More: the whole summary when the hero cut it, the profile's rating, then going to the show or season, and the collections it is part of. */
private fun LazyListScope.more(
    page: ItemPage,
    actions: TvPageActions,
    first: FocusRequester,
    summaryCut: Boolean,
    onSummary: () -> Unit,
    close: () -> Unit,
) {
    item(key = "title", contentType = PANEL_TITLE) { PanelTitle(stringResource(R.string.tv_item_more)) }
    if (summaryCut) {
        item(key = "summary", contentType = PANEL_ROW) { PanelRow(stringResource(R.string.tv_item_full_summary), icon = PleyIcons.Info, onClick = onSummary) }
    }
    var given = false
    if (page.rateable) {
        item(key = "rating", contentType = PANEL_HEADING) { PanelHeading(stringResource(R.string.tv_item_your_rating)) }
        val current = page.userRating
        item(key = "rating-none", contentType = PANEL_ROW) {
            PanelRow(stringResource(R.string.tv_item_no_rating), onClick = { actions.model.rate(null) }, checked = current == null, focusRequester = if (current == null) first else null)
        }
        if (current == null) given = true
        (1..STARS).forEach { stars ->
            val value = stars * 2
            val checked = current == value || (current != null && current == value - 1)
            val requester = if (!given && checked) first.also { given = true } else null
            item(key = "rating-$stars", contentType = PANEL_ROW) {
                PanelRow(
                    pluralStringResource(R.plurals.tv_item_stars, stars, stars.toString()),
                    onClick = { actions.model.rate(value) },
                    checked = checked,
                    icon = PleyIcons.Star,
                    focusRequester = requester,
                )
            }
        }
    }
    val context = page.context
    val show = context?.showId
    if (show != null) {
        val requester = if (!given) first.also { given = true } else null
        item(key = "show", contentType = PANEL_ROW) {
            PanelRow(stringResource(R.string.tv_item_action_go_to_show), icon = PleyIcons.Tv, focusRequester = requester, onClick = {
                close()
                actions.open(PleyKey.Item(show))
            })
        }
    }
    val season = context?.seasonId
    if (page.kind == ItemKind.Episode && season != null) {
        val requester = if (!given) first.also { given = true } else null
        item(key = "season", contentType = PANEL_ROW) {
            PanelRow(stringResource(R.string.tv_item_action_go_to_season), icon = PleyIcons.Layers, focusRequester = requester, onClick = {
                close()
                actions.open(PleyKey.Item(season))
            })
        }
    }
    page.partOf.forEach { collection ->
        val requester = if (!given) first.also { given = true } else null
        item(key = "collection-${collection.id}", contentType = PANEL_ROW) {
            PanelRow(stringResource(R.string.tv_item_part_of, collection.title), icon = PleyIcons.Layers, focusRequester = requester, onClick = {
                close()
                actions.open(PleyKey.Collection(collection.id))
            })
        }
    }
    if (!given) {
        item(key = "playlist", contentType = PANEL_ROW) {
            PanelRow(stringResource(R.string.tv_item_add_to_playlist), icon = PleyIcons.ListPlus, focusRequester = first, onClick = {
                close()
                actions.open(PleyKey.AddToPlaylist(listOf(page.id)))
            })
        }
    }
}

/** A tile's actions (a long press of Select, rule 6): what its action sheet offers. */
private fun LazyListScope.itemActionsList(panel: Panel.Actions, actions: TvPageActions, first: FocusRequester, close: () -> Unit) {
    val item = panel.item
    item(key = "title", contentType = PANEL_TITLE) { PanelTitle(item.title) }
    itemActions(item).forEachIndexed { i, action ->
        item(key = "action-${action.name}", contentType = PANEL_ROW) {
            val (label, icon) = actionLook(action)
            PanelRow(label, icon = icon, focusRequester = if (i == 0) first else null, onClick = {
                close()
                when (action) {
                    ItemAction.Play, ItemAction.Resume -> actions.model.play(item, fromStart = false)
                    ItemAction.FromStart -> actions.model.play(item, fromStart = true)
                    ItemAction.MarkWatched -> actions.model.setWatched(item.id, true)
                    ItemAction.MarkUnwatched -> actions.model.setWatched(item.id, false)
                    else -> destination(action, item)?.let(actions::open)
                }
            })
        }
    }
}

@Composable
private fun actionLook(action: ItemAction): Pair<String, PleyIcon> = when (action) {
    ItemAction.Play -> stringResource(R.string.tv_item_action_play) to PleyIcons.Play
    ItemAction.Resume -> stringResource(R.string.tv_item_action_resume) to PleyIcons.Play
    ItemAction.FromStart -> stringResource(R.string.tv_item_action_from_start) to PleyIcons.RotateCcw
    ItemAction.MarkWatched -> stringResource(R.string.tv_item_mark_watched) to PleyIcons.CircleCheck
    ItemAction.MarkUnwatched -> stringResource(R.string.tv_item_mark_unwatched) to PleyIcons.EyeOff
    ItemAction.RemoveFromContinueWatching -> stringResource(R.string.tv_item_remove_from_continue_watching) to PleyIcons.X
    ItemAction.AddToMyList -> stringResource(R.string.tv_item_add_to_my_list) to PleyIcons.BookmarkPlus
    ItemAction.RemoveFromMyList -> stringResource(R.string.tv_item_remove_from_my_list) to PleyIcons.BookmarkMinus
    ItemAction.AddToPlaylist -> stringResource(R.string.tv_item_add_to_playlist) to PleyIcons.ListPlus
    ItemAction.GoToShow -> stringResource(R.string.tv_item_action_go_to_show) to PleyIcons.Tv
    ItemAction.GoToSeason -> stringResource(R.string.tv_item_action_go_to_season) to PleyIcons.Layers
    ItemAction.Open -> stringResource(R.string.tv_item_action_open) to PleyIcons.ArrowRight
}

@Composable
private fun PanelTitle(text: String) {
    BasicText(
        text,
        style = PleyType.Tv.Title.copy(color = PleyColors.Fg),
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp),
    )
}

private const val STARS = 5

/** The content types of a panel's list, so a row scrolled in reuses one scrolled out. */
private const val PANEL_TITLE = "title"
private const val PANEL_HEADING = "heading"
private const val PANEL_ROW = "row"

internal const val TV_PANEL = "pley.tv.item.panel"
