package app.pley.android.ui.tv.shell

import android.content.Context
import android.graphics.Bitmap
import androidx.core.graphics.createBitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RadialGradient
import android.graphics.Shader
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import app.pley.android.common.DeviceProfile
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyImageLoader
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkInterceptor
import app.pley.android.designsystem.image.ArtworkPolicy
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.QualityBadge
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.SeeAll
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.TvRowColumn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberRested
import coil3.ImageLoader
import coil3.Uri
import coil3.asImage
import coil3.decode.DataSource
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.ImageFetchResult
import coil3.memory.MemoryCache
import coil3.request.Options
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlin.random.Random

/**
 * Debug builds only (TvActivity's `--ez demo true`): the TV frame with
 * made-up libraries, a profile and a Home made of the kit, with scenery
 * drawn on the device, so the rail, rows and focus can be tried and
 * compared with the design mockups without a server. The titles are the
 * mockups' own. Not a screen of the app: Home is P4's.
 */
object TvShellDemo {
    val source: TvShellSource = object : TvShellSource {
        override suspend fun startFacts() = StartFacts(server = "demo", signedIn = true)

        override val libraries: Flow<List<Library>> = flowOf(
            listOf(
                Library(1, "Movies", LibraryKind.Movies),
                Library(2, "Shows", LibraryKind.Shows),
                Library(3, "Music", LibraryKind.Music),
            ),
        )

        override val profile: Flow<RailProfile?> = flowOf(RailProfile("Anna", RailProfile.parseColor("#e07a5f")))

        override suspend fun keepUpdated() = Unit
    }

    /** Home made of the kit; every other screen is the usual placeholder. */
    val placeholder: @Composable (NavKey) -> Unit = { key -> if (key == PleyKey.Home) DemoHome() else Placeholder(key) }

    /** Pictures drawn on the device (`demo://…`), through pley's own sizing. */
    fun imageLoader(context: Context, profile: DeviceProfile): ImageLoader {
        val metrics = context.resources.displayMetrics
        return ImageLoader.Builder(context.applicationContext)
            .components {
                add(ArtworkInterceptor(ArtworkPolicy(profile.tier, maxOf(metrics.widthPixels, metrics.heightPixels))))
                add(SceneFetcher.Factory())
            }
            .memoryCache { MemoryCache.Builder().maxSizeBytes(PleyImageLoader.tvMemoryCacheBytes(profile.tier)).build() }
            .build()
    }
}

/** What the demo's info block and backdrop show for a tile. */
@Immutable
private data class DemoDetails(
    val title: String,
    val meta: List<String>,
    val rating: String?,
    val badges: List<String>,
    val summary: String,
    val scene: String,
) {
    val backdrop: ArtworkImage get() = ArtworkImage(listOf("demo://backdrop/$scene"))
}

private object DemoData {
    private fun still(scene: String) = ArtworkImage(listOf("demo://still/$scene"))

    private fun poster(scene: String) = ArtworkImage(listOf("demo://poster/$scene"))

    val hero = DemoDetails(
        "Driftline", listOf("2024", "TV-14", "58 min"), null, listOf("4K", "DV P8.1", "Atmos"),
        "The storm cuts Driftline off from the mainland, and Mara has one night to decide whether to trust the stranger who swam ashore with the harbour master's keys.",
        "driftline",
    )

    val continueWatching = listOf(
        ArtworkTileModel(1, "Driftline", "S2 · E5 · The Long Night", still("driftline"), progress = 0.55f),
        ArtworkTileModel(2, "Ashfall", "1h 12m left", still("ashfall"), progress = 0.4f),
        ArtworkTileModel(3, "The Lantern House", "S1 · E3 · Glasshouse", still("lantern-house"), progress = 0.2f),
        ArtworkTileModel(4, "Saltwind", "38 min left", still("saltwind"), progress = 0.72f),
        ArtworkTileModel(5, "Parallel Coast", "S4 · E2 · 31 min left", still("parallel-coast"), progress = 0.45f),
    )

    val movies = listOf(
        ArtworkTileModel(11, "Low Orbit", "2024", poster("low-orbit")),
        ArtworkTileModel(12, "Halcyon", "2025", poster("halcyon"), badges = listOf("4K")),
        ArtworkTileModel(13, "Northern Lights", "2024", poster("northern-lights"), watched = true),
        ArtworkTileModel(14, "Copper Sky", "2020", poster("copper-sky")),
        ArtworkTileModel(15, "Violet Hour", "2021", poster("violet-hour")),
        ArtworkTileModel(16, "The Quiet Harbor", "2023", poster("quiet-harbor")),
        ArtworkTileModel(17, "Glass Meridian", "2022", poster("glass-meridian")),
        ArtworkTileModel(18, "A Film With No Poster Yet", "2018", null),
    )

    val shows = listOf(
        ArtworkTileModel(21, "Overgrowth", "2 seasons", poster("overgrowth"), unwatched = 6),
        ArtworkTileModel(22, "Redline", "1 season", poster("redline"), watched = true),
        ArtworkTileModel(23, "White Valley", "3 seasons", poster("white-valley"), unwatched = 3),
        ArtworkTileModel(24, "Driftline", "2 seasons", poster("driftline"), unwatched = 14),
        ArtworkTileModel(25, "The Lantern House", "4 seasons", poster("lantern-house"), unwatched = 40),
        ArtworkTileModel(26, "Parallel Coast", "4 seasons", poster("parallel-coast"), unwatched = 6),
    )

    val details: Map<Long, DemoDetails> = buildMap {
        put(1L, hero)
        put(2L, DemoDetails("Ashfall", listOf("2025", "R", "2h 02m"), "7.4", listOf("4K", "DV P8.1"), "A volcanologist and her estranged brother race the ash cloud across three borders to reach the only lab that can read the mountain's warning.", "ashfall"))
        put(3L, DemoDetails("The Lantern House", listOf("2021", "TV-MA", "4 seasons"), "8.1", listOf("1080p"), "A family inherits a lighthouse that keeps the lamps of everyone who ever lived there burning.", "lantern-house"))
        put(4L, DemoDetails("Saltwind", listOf("2024", "PG-13", "1h 51m"), "6.9", listOf("4K", "HDR10+"), "Two rival sailmakers are stranded on the same salt flat after a storm takes their boats.", "saltwind"))
        put(5L, DemoDetails("Parallel Coast", listOf("2019", "TV-14", "4 seasons"), "7.8", listOf("1080p", "5.1"), "Two towns on facing shores share one ferry, one radio station and one secret.", "parallel-coast"))
        (movies + shows).forEach { tile ->
            if (tile.id !in this) {
                put(tile.id, DemoDetails(tile.title, listOf(tile.subtitle, "PG-13", "1h 48m"), "7.1", tile.badges, "A made-up film standing in for one of yours, so the frame has something to show.", sceneOf(tile)))
            }
        }
    }

    private fun sceneOf(tile: ArtworkTileModel): String =
        tile.image?.candidates?.firstOrNull()?.substringAfterLast('/') ?: tile.title.lowercase().replace(' ', '-')
}

/** The kit at work: a hero at rest, rows under it, and an info block that follows the focused tile once it rests. */
@Composable
private fun DemoHome() {
    val memory = rememberFocusMemory()
    var inRows by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf(DemoData.hero) }
    val shown = rememberRested(if (inRows) focused else DemoData.hero)
    val heroAlpha by animateFloatAsState(if (inRows) 0f else 1f, PleyMotion.reveal(), label = "hero")
    val see = SeeAll("See all") { }
    val onFocus: (ArtworkTileModel) -> Unit = {
        inRows = true
        focused = DemoData.details.getValue(it.id)
    }
    Box(Modifier.fillMaxSize()) {
        TvBackdrop(shown.backdrop, shown.title, scrim = if (inRows) BackdropScrim.Rows else BackdropScrim.Hero)
        InfoBlock(shown, Modifier.graphicsLayer { alpha = 1f - heroAlpha })
        TvRowColumn(
            heroHeight = HeroHeight,
            modifier = Modifier.fillMaxSize(),
        ) {
            // At rest the first header sits at y = 432, under the hero; in the rows it moves up to 172.
            item("hero") {
                Hero(
                    DemoData.hero,
                    memory = memory,
                    modifier = Modifier
                        .height(HeroHeight)
                        .wrapContentHeight(Alignment.Top, unbounded = true)
                        .graphicsLayer { alpha = heroAlpha }
                        .onFocusChanged { if (it.hasFocus) inRows = false },
                )
            }
            item("continue") {
                TvRow(
                    "continue", "Continue Watching", DemoData.continueWatching, TileKind.Landscape,
                    onOpen = { }, memory = memory, onFocus = onFocus,
                )
            }
            item("movies") {
                TvRow(
                    "movies", "Recently Added in Movies", DemoData.movies, TileKind.Poster,
                    onOpen = { }, memory = memory, seeAll = see, onFocus = onFocus,
                )
            }
            item("shows") {
                TvRow(
                    "shows", "Recently Added in Shows", DemoData.shows, TileKind.Poster,
                    onOpen = { }, memory = memory, onFocus = onFocus,
                )
            }
        }
    }
}

/** The hero, down to the first row's header at y = 432 less the gap between rows (design spec §2.2). */
private val HeroHeight = 432.dp - PleyTv.RowGap

/** The hero's stack at x = 96 (design spec §2.2): title, context, metadata, progress, summary, buttons. */
@Composable
private fun Hero(details: DemoDetails, memory: app.pley.android.ui.tv.kit.FocusMemory, modifier: Modifier) {
    Column(modifier.padding(start = PleyTv.Gutter, top = 58.dp)) {
        Box(Modifier.height(84.dp), contentAlignment = Alignment.BottomStart) {
            BasicText(details.title, style = PleyType.Tv.HeroTitle.copy(color = PleyColors.Fg), maxLines = 1)
        }
        Spacer(Modifier.height(12.dp))
        BasicText("S2 · E5 · The Long Night", style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1)
        Spacer(Modifier.height(8.dp))
        MetaLine(details)
        Spacer(Modifier.height(12.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(160.dp, 4.dp).drawBehind {
                    val r = CornerRadius(size.height / 2f)
                    drawRoundRect(PleyColors.Surface4, cornerRadius = r)
                    drawRoundRect(PleyColors.Accent, size = Size(size.width * 0.55f, size.height), cornerRadius = r)
                },
            )
            Spacer(Modifier.width(10.dp))
            BasicText("26 min left", style = PleyType.Tv.Caption.copy(color = PleyColors.Fg))
        }
        Spacer(Modifier.height(10.dp))
        BasicText(
            details.summary,
            style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg.copy(alpha = 0.85f)),
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.width(464.dp),
        )
        Spacer(Modifier.height(20.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap)) {
            TvPillButton("Resume", onClick = { }, icon = PleyIcons.Play, modifier = Modifier.initialFocus(memory))
            TvRoundButton("From start", PleyIcons.RotateCcw, onClick = { })
            TvRoundButton("Info", PleyIcons.Info, onClick = { })
            TvRoundButton("Watched", PleyIcons.CircleCheck, onClick = { })
        }
    }
}

/** The focused tile's compact block (design spec §4.1): title, metadata, two lines of summary. */
@Composable
private fun InfoBlock(details: DemoDetails, modifier: Modifier) {
    Column(modifier.padding(start = PleyTv.Gutter, top = 30.dp).width(464.dp)) {
        Box(Modifier.height(60.dp), contentAlignment = Alignment.BottomStart) {
            BasicText(details.title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 1)
        }
        Spacer(Modifier.height(12.dp))
        MetaLine(details)
        Spacer(Modifier.height(10.dp))
        BasicText(
            details.summary,
            style = PleyType.Tv.Body.copy(color = PleyColors.Fg.copy(alpha = 0.85f)),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** year · rating badge · runtime · ★ score · quality badges, in tvLabel muted with subtle dots. */
@Composable
private fun MetaLine(details: DemoDetails) {
    val label = PleyType.Tv.Label.copy(color = PleyColors.Muted)
    val dot = PleyType.Tv.Label.copy(color = PleyColors.Subtle)
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        details.meta.forEachIndexed { i, part ->
            if (i > 0) BasicText("·", style = dot)
            if (i == 1) QualityBadge(part, TileStyle.Tv) else BasicText(part, style = label, maxLines = 1)
        }
        details.rating?.let {
            BasicText("·", style = dot)
            BasicText("★ $it", style = label.copy(color = PleyColors.Fg))
        }
        if (details.badges.isNotEmpty()) {
            BasicText("·", style = dot)
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) { details.badges.forEach { QualityBadge(it, TileStyle.Tv) } }
        }
    }
}

/**
 * Scenery for the demo, drawn on the device: `demo://poster/<name>`,
 * `demo://still/<name>` and `demo://backdrop/<name>`, at the width pley's
 * interceptor asked for (`?w=`).
 */
private class SceneFetcher(private val uri: Uri) : Fetcher {
    override suspend fun fetch(): FetchResult {
        val width = uri.query?.split('&')?.firstOrNull { it.startsWith("w=") }?.removePrefix("w=")?.toIntOrNull() ?: 320
        val name = uri.path.orEmpty().trim('/')
        val height = when (uri.authority) {
            "poster" -> width * 3 / 2
            else -> width * 9 / 16
        }
        return ImageFetchResult(draw(name, width, height).asImage(), isSampled = false, dataSource = DataSource.DISK)
    }

    private fun draw(name: String, w: Int, h: Int): Bitmap {
        val palette = PALETTES[Math.floorMod(name.hashCode(), PALETTES.size)]
        val random = Random(name.hashCode())
        val bitmap = createBitmap(w, h)
        val canvas = Canvas(bitmap)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        val fw = w.toFloat()
        val fh = h.toFloat()
        paint.shader = LinearGradient(0f, 0f, 0f, fh, palette[0], palette[1], Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, fw, fh, paint)
        val sunX = fw * (0.55f + random.nextFloat() * 0.25f)
        val sunY = fh * (0.25f + random.nextFloat() * 0.2f)
        val sunR = minOf(fw, fh) * 0.07f
        paint.shader = RadialGradient(sunX, sunY, sunR * 4.5f, (palette[2] and 0xFFFFFF) or 0x55000000, palette[2] and 0xFFFFFF, Shader.TileMode.CLAMP)
        canvas.drawCircle(sunX, sunY, sunR * 4.5f, paint)
        paint.shader = null
        paint.color = palette[2]
        canvas.drawCircle(sunX, sunY, sunR, paint)
        listOf(0.55f, 0.66f, 0.78f).forEachIndexed { i, base ->
            val path = Path().apply {
                moveTo(0f, base * fh)
                var x = 0f
                while (x < fw) {
                    x += fw * (0.06f + random.nextFloat() * 0.1f)
                    lineTo(x, base * fh - random.nextFloat() * fh * (0.12f - i * 0.03f))
                }
                lineTo(fw, fh)
                lineTo(0f, fh)
                close()
            }
            paint.color = palette[3 + i]
            canvas.drawPath(path, paint)
        }
        return bitmap
    }

    class Factory : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? =
            if (data.scheme == "demo") SceneFetcher(data) else null
    }

    private companion object {
        /** Sky top, sky bottom, sun, three hills. */
        val PALETTES = listOf(
            intArrayOf(0xFF1C2A4A.toInt(), 0xFF0A1022.toInt(), 0xFFE6F0FF.toInt(), 0xFF2A3C64.toInt(), 0xFF1A2846.toInt(), 0xFF0C1428.toInt()),
            intArrayOf(0xFF5E1A16.toInt(), 0xFF1A0705.toInt(), 0xFFF4C9A0.toInt(), 0xFF6A201A.toInt(), 0xFF451310.toInt(), 0xFF1C0705.toInt()),
            intArrayOf(0xFF3E2A55.toInt(), 0xFF1A1226.toInt(), 0xFFF6D49A.toInt(), 0xFF6A4A7E.toInt(), 0xFF46315A.toInt(), 0xFF22172E.toInt()),
            intArrayOf(0xFF9A4A22.toInt(), 0xFF3A1A0C.toInt(), 0xFFFBEFD0.toInt(), 0xFFB0582A.toInt(), 0xFF7A3A1C.toInt(), 0xFF3A1A0C.toInt()),
            intArrayOf(0xFF2A5A6E.toInt(), 0xFF0E2630.toInt(), 0xFFF2E0B4.toInt(), 0xFF2F6276.toInt(), 0xFF1E4252.toInt(), 0xFF0E2029.toInt()),
            intArrayOf(0xFF2E1E4E.toInt(), 0xFF120A22.toInt(), 0xFFFFB8E0.toInt(), 0xFF4A3070.toInt(), 0xFF2E1E48.toInt(), 0xFF140C22.toInt()),
            intArrayOf(0xFF7A4A20.toInt(), 0xFF2A160A.toInt(), 0xFFFFE2B0.toInt(), 0xFF8A5428.toInt(), 0xFF5C3518.toInt(), 0xFF2A160A.toInt()),
        )
    }
}
