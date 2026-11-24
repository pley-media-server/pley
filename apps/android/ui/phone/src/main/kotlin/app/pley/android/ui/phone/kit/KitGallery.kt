package app.pley.android.ui.phone.kit

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * Every part of the phone kit on one page, with made-up titles whose
 * placeholders show Pley's mark: for looking at in debug builds (the
 * placeholders open it) and for screenshots against the design's mockups.
 */
@Composable
internal fun KitGallery(onBack: () -> Unit) {
    val scroll = rememberScrollState()
    val skeleton = rememberSkeleton(animated = true)
    val margin = LocalPhoneMetrics.current.margin
    var sheet by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    var rating by rememberSaveable { mutableStateOf<Int?>(7) }
    var segment by rememberSaveable { mutableIntStateOf(1) }
    var unwatched by rememberSaveable { mutableStateOf(true) }
    val offline = remember { mutableStateOf(false) }
    val toasts = remember { ToastHostState() }
    Box(Modifier.fillMaxSize()) {
        CompositionLocalProvider(LocalOffline provides offline) {
            Column(Modifier.fillMaxSize()) {
                PleyTopAppBar(title = "Phone kit", onBack = onBack, scrolled = { scroll.value > 0 })
                Column(Modifier.weight(1f).verticalScroll(scroll).padding(bottom = 32.dp)) {
                    HubRow(HubRowModel("continue", "Continue Watching", TileKind.Landscape, continueWatching), onOpen = {}, skeleton = skeleton, onLongPress = { sheet = true })
                    Gap()
                    HubRow(HubRowModel("movies", "Recently Added in Movies", TileKind.Poster, movies), onOpen = {}, skeleton = skeleton, onTitleClick = {})
                    Gap()
                    HubRow(HubRowModel("loading", "Recently Added in Shows", TileKind.Poster, null), onOpen = {}, skeleton = skeleton)
                    Gap()
                    Section("Buttons", margin)
                    Column(Modifier.padding(horizontal = margin), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        PrimaryButton("Resume", onClick = {}, icon = PleyIcons.Play, modifier = Modifier.fillMaxWidth())
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            TonalButton("Enter an address", onClick = {})
                            PleyTextButton("More", onClick = {})
                        }
                        Row {
                            ActionButton(PleyIcons.RotateCcw, "From start", onClick = {})
                            ActionButton(PleyIcons.CircleCheck, "Watched", onClick = {}, selected = true)
                            ActionButton(PleyIcons.ListPlus, "Playlist", onClick = {})
                            ActionButton(PleyIcons.Film, "Trailer", onClick = {}, enabled = false)
                        }
                    }
                    Section("Chips and choices", margin)
                    FlowRow(Modifier.padding(horizontal = margin), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        PleyChip("Recently added", selected = false, onClick = {}, icon = PleyIcons.ArrowUpDown)
                        PleyChip("Unwatched", selected = unwatched, onClick = { unwatched = !unwatched })
                        PleyChip("Genre", selected = false, onClick = {}, dropdown = true)
                        PleyChip("Year", selected = false, onClick = {}, dropdown = true)
                    }
                    Spacer(Modifier.height(16.dp))
                    SegmentedControl(listOf("Recommended", "Library", "Collections"), segment, { segment = it }, Modifier.padding(horizontal = margin))
                    Spacer(Modifier.height(16.dp))
                    Row(Modifier.padding(horizontal = margin)) {
                        StarRating(rating, { rating = it })
                    }
                    Section("Profiles", margin)
                    Row(Modifier.padding(horizontal = margin), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        listOf("Alex", "Sam", "Robin", "Kai", "Noor", "Jo").forEach { Avatar(it, size = 40.dp) }
                    }
                    Section("Feedback", margin)
                    Column(Modifier.padding(horizontal = margin), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            TonalButton("Open a sheet", onClick = { sheet = true })
                            TonalButton("Ask first", onClick = { confirm = true })
                        }
                        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            TonalButton("Toast", onClick = { toasts.show("Marked as watched", actionLabel = "Undo", onAction = {}) })
                            TonalButton("Offline", onClick = { offline.value = !offline.value })
                        }
                        BusyLine("Finish signing in in your browser.")
                    }
                    EmptyState("No matches", "Nothing in this library matches the current filters.", action = { TonalButton("Clear filters", onClick = {}) })
                    ErrorState("Couldn't load your home screen", java.net.ConnectException(), onRetry = {})
                    Section("Grid", margin)
                    ItemGrid(
                        items = movies,
                        kind = TileKind.Poster,
                        onOpen = {},
                        skeleton = skeleton,
                        modifier = Modifier.height(720.dp),
                    )
                }
            }
        }
        ToastHost(toasts, Modifier.align(Alignment.BottomCenter))
    }
    if (sheet) {
        PleySheet(onDismiss = { sheet = false }, title = "Northern Lights") {
            SheetAction("Play from the start", onClick = { sheet = false }, icon = PleyIcons.RotateCcw)
            SheetAction("Mark as watched", onClick = { sheet = false }, icon = PleyIcons.CircleCheck)
            SheetAction("Add to playlist", onClick = { sheet = false }, icon = PleyIcons.ListPlus)
            SheetAction("Remove from Continue Watching", onClick = { sheet = false }, icon = PleyIcons.X, danger = true)
        }
    }
    if (confirm) {
        ConfirmDialog(
            title = "Sign out?",
            body = "Pley forgets this server's sign-in on this phone.",
            confirmLabel = "Sign out",
            onConfirm = { confirm = false },
            onDismiss = { confirm = false },
            destructive = true,
        )
    }
}

@Composable
private fun Gap() = Spacer(Modifier.height(PleyPhone.SectionGap - PRESS_ROOM))

@Composable
private fun Section(title: String, margin: androidx.compose.ui.unit.Dp) {
    Spacer(Modifier.height(PleyPhone.SectionGap))
    Text(title, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg, modifier = Modifier.padding(horizontal = margin))
    Spacer(Modifier.height(PleyPhone.HeaderToContent))
}

private val continueWatching = listOf(
    ArtworkTileModel(1, "Driftline", "S2 · E5 · 26 min left", progress = 0.55f, mark = MarkGlyph.Shows),
    ArtworkTileModel(2, "Ashfall", "1h 12m left", progress = 0.4f, mark = MarkGlyph.Movies),
    ArtworkTileModel(3, "Halcyon", "48 min left", progress = 0.7f, mark = MarkGlyph.Movies),
)

private val movies = listOf(
    ArtworkTileModel(11, "Low Orbit", "2024", watched = true),
    ArtworkTileModel(12, "Halcyon", "2025", badges = listOf("4K", "DV")),
    ArtworkTileModel(13, "Northern Lights", "2024"),
    ArtworkTileModel(14, "Glass Meridian", "2022", progress = 0.3f),
    ArtworkTileModel(15, "Saltwind", "2021"),
    ArtworkTileModel(16, "The Quiet Harbor", "2023", unwatched = 3),
    ArtworkTileModel(17, "Copper Sky", "2020"),
    ArtworkTileModel(18, "The Fen", "2019"),
    ArtworkTileModel(19, "Overgrowth", "2024"),
).map { it.copy(mark = MarkGlyph.Movies) }
