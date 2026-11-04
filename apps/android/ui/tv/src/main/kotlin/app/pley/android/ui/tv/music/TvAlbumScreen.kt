package app.pley.android.ui.tv.music

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.sentenceOf
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.screens.PleyKey
import app.pley.android.screens.music.AlbumPage
import app.pley.android.screens.music.AlbumRow
import app.pley.android.screens.music.AlbumViewModel
import app.pley.android.screens.music.MusicEvent
import app.pley.android.screens.music.MusicPageState
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * An album on TV (MusicViews.tsx AlbumView; design spec §4.10): the art
 * and what it is on the left (the artist, a link to their page; the year,
 * the tracks and their length; the profile's rating), Play and Shuffle
 * under it, and the tracks on the right with disc headers, the one
 * playing marked. Select on a track plays the album from it, or pauses
 * the track playing. Playing opens Now Playing; Back comes here.
 *
 * Focus starts on Play, or on the track the album was opened from.
 */
@Composable
internal fun TvAlbumScreen(albumId: Long, trackId: Long?) {
    val model: AlbumViewModel = viewModel(factory = AlbumViewModel.factory(albumId, trackId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    var problem by remember { mutableStateOf<String?>(null) }
    val nothing = stringResource(R.string.tv_music_nothing_to_play)
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                MusicEvent.Played -> navigator.open(PleyKey.NowPlaying)
                MusicEvent.NothingToPlay -> problem = nothing
                is MusicEvent.Failed -> problem = sentenceOf(event.error)
            }
        }
    }
    Box(Modifier.fillMaxSize().testTag(TV_ALBUM_PAGE)) {
        when (val s = state) {
            MusicPageState.Loading -> AlbumSkeleton()
            MusicPageState.NotFound -> MusicMessage(
                title = stringResource(R.string.tv_music_album_not_found),
                body = stringResource(R.string.tv_music_not_found_body),
                action = stringResource(R.string.tv_music_back),
                onAction = navigator::back,
            )
            is MusicPageState.Failed -> MusicMessage(
                title = failedTitle(s.error, stringResource(R.string.tv_music_album_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_music_retry),
                onAction = model::retry,
            )
            is MusicPageState.Ready -> {
                val art = remember(s.page.posterUrl, s.page.origin) { ArtworkImage.of(s.page.posterUrl, s.page.origin) }
                FaintArt(art, s.page.title, alpha = 0.14f)
                AlbumContent(
                    s.page,
                    art,
                    current = s.current,
                    playing = s.playing,
                    highlight = trackId,
                    problem = problem,
                    onPlay = { model.play(0) },
                    onShuffle = model::shuffle,
                    onTrack = model::playOrToggle,
                    onArtist = { id -> navigator.open(PleyKey.Artist(id)) },
                    onNowPlaying = { navigator.open(PleyKey.NowPlaying) },
                )
            }
        }
    }
}

@Composable
private fun AlbumContent(
    page: AlbumPage,
    art: ArtworkImage?,
    current: Long?,
    playing: Boolean,
    highlight: Long?,
    problem: String?,
    onPlay: () -> Unit,
    onShuffle: () -> Unit,
    onTrack: (Int) -> Unit,
    onArtist: (Long) -> Unit,
    onNowPlaying: () -> Unit,
) {
    val memory = rememberFocusMemory()
    val highlighted = highlight?.takeIf { id -> page.rows.any { it is AlbumRow.Track && it.id == id } }
    Row(Modifier.fillMaxSize().padding(top = TOP)) {
        // The album: x = 96, the art 220 wide, room beside it for a third button (Now playing).
        Column(
            Modifier
                .padding(start = PleyTv.Gutter)
                .width(COLUMN)
                .fillMaxHeight()
                .focusRestorer()
                .focusGroup(),
        ) {
            Box(Modifier.size(ART).clip(PleyShape.TileTv).background(PleyColors.Surface2)) {
                Artwork(art, page.title, Modifier.matchParentSize())
            }
            Spacer(Modifier.height(12.dp))
            BasicText(page.title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
            val artistId = page.artistId
            if (page.artist != null) {
                if (artistId != null) {
                    // The link's own padding sits outside the column's edge, so its text lines up with the title's.
                    TvTextButton(
                        page.artist!!,
                        onClick = { onArtist(artistId) },
                        color = PleyColors.AccentBright,
                        link = true,
                        modifier = Modifier.padding(top = 2.dp).offset(x = (-16).dp).rememberedFocus(memory, "artist"),
                    )
                } else {
                    BasicText(page.artist!!, style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
                }
            }
            val meta = listOfNotNull(page.year?.toString(), page.meta.ifEmpty { null }).joinToString(" · ")
            if (meta.isNotEmpty()) BasicText(meta, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), modifier = Modifier.padding(top = 4.dp))
            page.userRating?.let { rating -> Stars(rating, Modifier.padding(top = 6.dp)) }
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap), verticalAlignment = Alignment.Top) {
                TvPillButton(
                    stringResource(R.string.tv_music_play),
                    onClick = onPlay,
                    icon = PleyIcons.Play,
                    enabled = page.tracksLoading || page.tracks.isNotEmpty(),
                    modifier = Modifier.rememberedFocus(memory, "play", initial = highlighted == null),
                )
                TvRoundButton(stringResource(R.string.tv_music_shuffle), PleyIcons.Shuffle, onClick = onShuffle, onArt = false, enabled = page.tracks.isNotEmpty(), modifier = Modifier.rememberedFocus(memory, "shuffle"))
                if (current != null) {
                    TvRoundButton(stringResource(R.string.tv_music_now_playing), PleyIcons.AudioLines, onClick = onNowPlaying, onArt = false, modifier = Modifier.rememberedFocus(memory, "now"))
                }
            }
            if (problem != null) BasicText(problem, style = PleyType.Tv.Caption.copy(color = PleyColors.DangerText), modifier = Modifier.padding(top = 8.dp))
        }
        Spacer(Modifier.width(24.dp))
        // The tracks: from x = 400 to the right safe edge.
        val list = rememberLazyListState()
        // Opened from a track: it is scrolled to, so it is there to take focus.
        val at = page.rows.indexOfFirst { it is AlbumRow.Track && it.id == highlighted }
        LaunchedEffect(at) { if (at > 3) list.scrollToItem(at - 3) }
        // Coming in (from the left, or the page's first focus): the track it was opened from, the one playing, else the first.
        val entry = remember { FocusRequester() }
        val entryId = highlighted ?: current?.takeIf { id -> page.tracks.any { it.id == id } } ?: page.tracks.firstOrNull()?.id
        LazyColumn(
            Modifier
                .fillMaxSize()
                .padding(end = TvSafeArea.Horizontal)
                .focusRestorer(entry)
                .heldKeyStaysIn(HeldEnds.UpDown)
                .focusGroup()
                .testTag(TV_ALBUM_TRACKS),
            state = list,
            contentPadding = PaddingValues(bottom = 120.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            if (page.tracksLoading) {
                items(8, key = { "skeleton-$it" }) {
                    Box(Modifier.padding(vertical = 4.dp).size(480.dp, 36.dp).background(PleyColors.Surface2, PleyShape.Field))
                }
            } else if (page.tracks.isEmpty()) {
                item(key = "empty") { BasicText(stringResource(R.string.tv_music_no_tracks), style = PleyType.Tv.Body.copy(color = PleyColors.Muted)) }
            }
            items(page.rows, key = { it.key }, contentType = { it.javaClass }) { row ->
                when (row) {
                    is AlbumRow.Disc -> BasicText(
                        stringResource(R.string.tv_music_disc, row.number),
                        style = PleyType.Tv.Label.copy(color = PleyColors.Subtle),
                        modifier = Modifier.padding(start = 12.dp, top = 14.dp, bottom = 4.dp),
                    )
                    is AlbumRow.Track -> TvTrackLine(
                        number = row.number,
                        title = row.title,
                        subtitle = row.artist,
                        duration = row.duration,
                        current = current == row.id,
                        playing = current == row.id && playing,
                        highlighted = highlighted == row.id,
                        onClick = { onTrack(row.position) },
                        modifier = Modifier
                            .rememberedFocus(memory, "track-${row.id}", initial = highlighted == row.id)
                            .then(if (row.id == entryId) Modifier.focusRequester(entry) else Modifier),
                    )
                }
            }
        }
    }
}

/** The profile's rating as stars (0 to 10, half stars): shown, changed on the web or the phone. */
@Composable
internal fun Stars(rating: Int, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        (1..5).forEach { star ->
            val filled = rating >= star * 2 || rating == star * 2 - 1
            Icon(PleyIcons.Star, contentDescription = null, size = 16.dp, tint = if (filled) PleyColors.Accent else PleyColors.Surface4)
        }
    }
}

@Composable
private fun AlbumSkeleton() {
    Row(Modifier.fillMaxSize().padding(start = PleyTv.Gutter, top = TOP)) {
        Column {
            Box(Modifier.size(ART).background(PleyColors.Surface2, PleyShape.TileTv))
            Spacer(Modifier.height(14.dp))
            Box(Modifier.size(180.dp, 22.dp).background(PleyColors.Surface2, PleyShape.Badge))
            Spacer(Modifier.height(8.dp))
            Box(Modifier.size(120.dp, 16.dp).background(PleyColors.Surface2, PleyShape.Badge))
        }
        Spacer(Modifier.width(40.dp))
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            repeat(8) { Box(Modifier.size(480.dp, 34.dp).background(PleyColors.Surface2, PleyShape.Field)) }
        }
    }
}

private val TOP = 40.dp
private val ART = 220.dp

/** The album's column: the art, and Play, Shuffle and Now playing under it. */
private val COLUMN = 280.dp

internal const val TV_ALBUM_PAGE = "pley.tv.album"
internal const val TV_ALBUM_TRACKS = "pley.tv.album.tracks"
