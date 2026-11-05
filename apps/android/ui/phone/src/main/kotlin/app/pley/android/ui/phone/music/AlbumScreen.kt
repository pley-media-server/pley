package app.pley.android.ui.phone.music

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.screens.PleyKey
import app.pley.android.screens.music.AlbumRow
import app.pley.android.screens.music.AlbumViewModel
import app.pley.android.screens.music.MusicEvent
import app.pley.android.screens.music.MusicPageState
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.StarRating
import app.pley.android.ui.phone.library.joined
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * An album's page (MusicViews.tsx AlbumView; design spec §4.10): the art,
 * the artist (a link to their page), the year, how many tracks and how
 * long, the profile's rating, Play and Shuffle, then the tracks with disc
 * headers. The track playing is marked; tapping it pauses or plays on,
 * tapping another plays the album from there. Opened from a track, that
 * track is tinted and scrolled to.
 */
@Composable
internal fun AlbumScreen(albumId: Long, trackId: Long?) {
    val model: AlbumViewModel = viewModel(factory = AlbumViewModel.factory(albumId, trackId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                is MusicEvent.Failed -> shell.report(event.error)
                MusicEvent.NothingToPlay -> shell.toasts.show(resources.getString(R.string.music_nothing_to_play))
                MusicEvent.Played -> Unit
            }
        }
    }
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(ALBUM_PAGE)) {
        val ready = state as? MusicPageState.Ready
        val pastHeader by remember { derivedStateOf { list.firstVisibleItemIndex > 0 } }
        PleyTopAppBar(
            title = if (pastHeader) ready?.page?.title.orEmpty() else "",
            onBack = navigator::back,
            scrolled = { list.canScrollBackward },
        )
        when (val s = state) {
            MusicPageState.Loading -> MusicHeaderSkeleton(round = false)
            MusicPageState.NotFound -> EmptyState(
                title = stringResource(R.string.music_album_not_found),
                body = stringResource(R.string.music_not_found_body),
                icon = PleyIcons.Music,
            )
            is MusicPageState.Failed -> ErrorState(stringResource(R.string.music_album_failed), s.error, onRetry = model::retry)
            is MusicPageState.Ready -> {
                ReportLoad(s.status)
                val page = s.page
                val metrics = LocalPhoneMetrics.current
                // Opened from a track: it scrolls into view once the tracks are there.
                var scrolled by rememberSaveable { mutableStateOf(trackId == null) }
                LaunchedEffect(page.rows.size) {
                    if (scrolled || page.tracksLoading) return@LaunchedEffect
                    val at = page.rows.indexOfFirst { it is AlbumRow.Track && it.id == trackId }
                    if (at >= 0) list.animateScrollToItem(at + HEADER_ITEMS + (if (page.summary != null) 1 else 0), scrollOffset = -200)
                    scrolled = true
                }
                LazyColumn(
                    state = list,
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = metrics.marginStart - 8.dp, end = metrics.marginEnd - 8.dp, bottom = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    item(key = "header", contentType = "header") {
                        val art = remember(page.posterUrl, page.origin) { ArtworkImage.of(page.posterUrl, page.origin) }
                        Box(Modifier.padding(horizontal = 8.dp)) {
                            MusicHeader(
                                art = art,
                                round = false,
                                kind = stringResource(R.string.music_album),
                                title = page.title,
                                lines = listOf(listOfNotNull(page.year?.toString(), page.metaTexts.joined(resources).ifEmpty { null }).joinToString(" · ")).filter { it.isNotEmpty() },
                                busy = false,
                                playEnabled = page.tracksLoading || page.tracks.isNotEmpty(),
                                onPlay = { model.play(0) },
                                onShuffle = model::shuffle,
                                playOn = CastTarget(itemId = albumId, shuffles = true).takeIf { page.tracks.isNotEmpty() },
                                extra = {
                                    val artistId = page.artistId
                                    if (page.artist != null) {
                                        Spacer(Modifier.height(4.dp))
                                        Text(
                                            page.artist!!,
                                            style = MaterialTheme.typography.titleMedium,
                                            color = if (artistId != null) PleyColors.AccentBright else PleyColors.Fg,
                                            modifier = if (artistId != null) {
                                                Modifier.clickable(role = Role.Button) { navigator.open(PleyKey.Artist(artistId)) }
                                            } else {
                                                Modifier
                                            },
                                        )
                                    }
                                },
                            )
                        }
                    }
                    item(key = "rating", contentType = "rating") {
                        StarRating(page.userRating, onChange = model::rate, modifier = Modifier.padding(bottom = 8.dp))
                    }
                    if (page.summary != null) {
                        item(key = "summary", contentType = "summary") {
                            Box(Modifier.widthIn(max = MAX_WIDTH).padding(horizontal = 8.dp, vertical = 8.dp)) { Summary(page.summary!!) }
                        }
                    }
                    if (!page.tracksLoading && page.tracks.isEmpty()) {
                        item(key = "empty", contentType = "empty") {
                            Text(stringResource(R.string.music_no_tracks), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.padding(24.dp))
                        }
                    }
                    items(page.rows, key = { it.key }, contentType = { it.javaClass }) { row ->
                        Box(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth()) {
                            when (row) {
                                is AlbumRow.Disc -> DiscHeader(row.number)
                                is AlbumRow.Track -> TrackLine(
                                    row,
                                    current = s.current == row.id,
                                    playing = s.current == row.id && s.playing,
                                    highlighted = trackId == row.id,
                                    onClick = { model.playOrToggle(row.position) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/** The list's items above the tracks besides the summary: the header and the rating. */
private const val HEADER_ITEMS = 2

private val MAX_WIDTH = 720.dp

internal const val ALBUM_PAGE = "pley.album.page"
