package app.pley.android.ui.tv.person

import android.content.res.Resources
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.resolve
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.CreditRole
import app.pley.android.model.format.FormatWords
import app.pley.android.screens.PleyKey
import app.pley.android.screens.person.CreditTile
import app.pley.android.screens.person.PersonPage
import app.pley.android.screens.person.PersonUiState
import app.pley.android.screens.person.PersonViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvMessage
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * A person's page on TV (design spec §4.9): the photo as a 160 dp circle
 * at (96, 58), the name in tvPageTitle beside it and what of theirs is in
 * the libraries; then a row per role (Acting, Directing…), captioned with
 * the year and the part. Focus starts on the first title.
 */
@Composable
internal fun TvPersonScreen(personId: Long) {
    val model: PersonViewModel = viewModel(factory = PersonViewModel.factory(personId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val actions = tileActions()
    Box(Modifier.fillMaxSize().testTag(TV_PERSON_PAGE)) {
        when (val s = state) {
            PersonUiState.Loading -> PersonHeader(null)
            PersonUiState.NotFound -> TvMessage(
                title = stringResource(R.string.tv_person_not_found),
                body = stringResource(R.string.tv_person_not_found_body),
                action = stringResource(R.string.tv_item_back),
                onAction = navigator::back,
                icon = PleyIcons.UserRound,
            )
            is PersonUiState.Failed -> TvMessage(
                title = failedTitle(s.error, stringResource(R.string.tv_person_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_item_retry),
                onAction = model::retry,
            )
            is PersonUiState.Ready -> {
                PersonHeader(s.page)
                PersonRows(s.page, onOpen = { navigator.open(PleyKey.Item(it)) }, onActions = { actions.open(it, continueWatching = false) })
            }
        }
    }
}

/** The photo, the name and the count; still blocks while loading. */
@Composable
private fun PersonHeader(page: PersonPage?) {
    Row(Modifier.padding(start = PleyTv.Gutter, top = 58.dp, end = TvSafeArea.Horizontal), verticalAlignment = Alignment.CenterVertically) {
        val photo = remember(page?.photoUrl, page?.origin) { page?.let { ArtworkImage.of(it.photoUrl, it.origin) } }
        Box(Modifier.size(PhotoSize).clip(PleyShape.Circle).background(PleyColors.Surface2), contentAlignment = Alignment.Center) {
            if (page != null) {
                Artwork(photo, page.name, Modifier.matchParentSize())
                if (photo == null) BasicText(initials(page.name), style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg.copy(alpha = 0.8f)))
            }
        }
        Spacer(Modifier.width(32.dp))
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (page == null) {
                Box(Modifier.size(280.dp, 38.dp).background(PleyColors.Surface2, PleyShape.Badge))
                Box(Modifier.size(200.dp, 20.dp).background(PleyColors.Surface2, PleyShape.Badge))
            } else {
                BasicText(page.name, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (page.countTexts.isNotEmpty()) {
                    BasicText(stringResource(R.string.tv_person_in_libraries, page.countTexts.asString()), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
                }
            }
        }
    }
}

@Composable
private fun PersonRows(page: PersonPage, onOpen: (Long) -> Unit, onActions: (Long) -> Unit) {
    val memory = rememberFocusMemory()
    val words = formatWords()
    val resources = LocalResources.current
    val rows = remember(page, words, resources) {
        page.roles.map { row -> row.role to row.credits.map { it.tile(page.origin, words, resources) } }
    }
    TvRowColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(top = RowsTop, bottom = 300.dp),
        headerY = RowsTop,
    ) {
        rows.forEachIndexed { index, (role, tiles) ->
            item(key = role.name, contentType = TileKind.Poster) {
                TvRow(
                    rowKey = role.name,
                    title = roleTitle(role),
                    tiles = tiles,
                    kind = TileKind.Poster,
                    onOpen = { onOpen(it.id) },
                    onActions = { onActions(it.id) },
                    memory = memory,
                    initialFocus = index == 0,
                )
            }
        }
    }
}

@Composable
private fun roleTitle(role: CreditRole): String = stringResource(
    when (role) {
        CreditRole.Director -> R.string.tv_person_directing
        CreditRole.Creator -> R.string.tv_person_creating
        CreditRole.Writer -> R.string.tv_person_writing
        CreditRole.Producer -> R.string.tv_person_producing
        else -> R.string.tv_person_acting
    },
)

/** "Idris Vale" → "IV". */
private fun initials(name: String): String =
    name.split(' ').filter { it.isNotBlank() }.take(2).joinToString("") { it.first().uppercase() }

private val PhotoSize = 160.dp

/** The first row's header, under the photo (58 + 160 + room). */
private val RowsTop = 240.dp

internal const val TV_PERSON_PAGE = "pley.tv.person"

/** The credit's poster, captioned with the year and the roles in the language of [resources]: "2019 · as Neo". */
private fun CreditTile.tile(origin: String, words: FormatWords, resources: Resources): ArtworkTileModel {
    val caption = listOf(item.year?.toString().orEmpty(), roles.joinToString(", ") { it.resolve(resources) }).filter { it.isNotEmpty() }.joinToString(" · ")
    return ArtworkTileModel.of(item, TileKind.Poster, origin, words).copy(subtitle = caption)
}
