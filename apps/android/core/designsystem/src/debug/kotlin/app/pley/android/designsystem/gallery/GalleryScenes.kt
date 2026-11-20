package app.pley.android.designsystem.gallery

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RadialGradient
import android.graphics.Shader
import android.graphics.Typeface
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import coil3.ImageLoader
import coil3.Uri
import coil3.asImage
import coil3.decode.DataSource
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.ImageFetchResult
import coil3.request.Options
import kotlin.random.Random

/**
 * Pictures for the gallery, drawn on the device so it needs no server:
 * scenery standing in for posters and stills, as the design mockups use.
 * `gallery://poster/<name>?w=320` and `gallery://still/<name>?w=480`; the
 * width is the one [ArtworkInterceptor][app.pley.android.designsystem.image.ArtworkInterceptor]
 * asked for, so the gallery shows the real buckets. `gallery://missing/…`
 * fails, as a 404 would.
 */
internal class GallerySceneFetcher(private val uri: Uri) : Fetcher {
    override suspend fun fetch(): FetchResult {
        val kind = uri.authority
        if (kind == "missing") throw IllegalStateException("No such picture: $uri")
        val width = uri.query?.split('&')?.firstOrNull { it.startsWith("w=") }?.removePrefix("w=")?.toIntOrNull() ?: 320
        val name = uri.path.orEmpty().trim('/')
        val bitmap = when (kind) {
            "poster" -> Scenes.draw(name, width, width * 3 / 2, poster = true)
            "square" -> Scenes.draw(name, width, width, poster = false)
            else -> Scenes.draw(name, width, width * 9 / 16, poster = false)
        }
        return ImageFetchResult(bitmap.asImage(), isSampled = false, dataSource = DataSource.DISK)
    }

    class Factory : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? =
            if (data.scheme == "gallery") GallerySceneFetcher(data) else null
    }
}

/** A scene's colors and its title as printed on the poster. */
internal data class Scene(
    val skyTop: Int,
    val skyBottom: Int,
    val sun: Int,
    val hills: List<Int>,
    val stars: Boolean = false,
    val title: String = "",
    val year: String = "",
    val face: Typeface = Typeface.create(Typeface.SANS_SERIF, Typeface.BOLD),
    val sunX: Float = 0.72f,
    val sunY: Float = 0.32f,
)

internal object Scenes {
    private val serif = Typeface.create(Typeface.SERIF, Typeface.BOLD)
    private val condensed = Typeface.create("sans-serif-condensed", Typeface.BOLD)
    private val heavy = Typeface.create("sans-serif-black", Typeface.NORMAL)

    val all: Map<String, Scene> = mapOf(
        "quiet-harbor" to Scene(0xFF2B5566.toInt(), 0xFF10202A.toInt(), 0xFFF3DFB0.toInt(), listOf(0xFF2F5C6E.toInt(), 0xFF1E3E4C.toInt(), 0xFF0F2029.toInt()), title = "The Quiet Harbor", year = "2023", face = serif),
        "glass-meridian" to Scene(0xFF3A1A4A.toInt(), 0xFF140A1E.toInt(), 0xFFF6A8D0.toInt(), listOf(0xFF4E2560.toInt(), 0xFF32173F.toInt(), 0xFF1A0C22.toInt()), stars = true, title = "Glass Meridian", year = "2022", face = heavy),
        "low-orbit" to Scene(0xFF1A2442.toInt(), 0xFF0A0F1E.toInt(), 0xFFDCEBFA.toInt(), listOf(0xFF2A3A5E.toInt(), 0xFF1B2744.toInt(), 0xFF0E1528.toInt()), stars = true, title = "Low Orbit", year = "2024", face = heavy),
        "halcyon" to Scene(0xFF1F4E5E.toInt(), 0xFF0D2530.toInt(), 0xFFF2E4B6.toInt(), listOf(0xFF22505F.toInt(), 0xFF173A46.toInt(), 0xFF0C1E26.toInt()), title = "HALCYON", year = "2025", face = serif, sunX = 0.6f),
        "northern-lights" to Scene(0xFF2C5C7A.toInt(), 0xFF112A3A.toInt(), 0xFFF0F4FA.toInt(), listOf(0xFF5B87A6.toInt(), 0xFF2E5470.toInt(), 0xFF142B3B.toInt()), title = "NORTHERN LIGHTS", year = "2024", face = condensed, sunX = 0.8f, sunY = 0.5f),
        "overgrowth" to Scene(0xFF4A6A5E.toInt(), 0xFF1C2A24.toInt(), 0xFFF1F3EA.toInt(), listOf(0xFF5C7C6E.toInt(), 0xFF3C5A4C.toInt(), 0xFF1E2E26.toInt()), title = "Overgrowth", year = "2024–", face = heavy),
        "redline" to Scene(0xFF5A1414.toInt(), 0xFF1E0606.toInt(), 0xFFF6C08A.toInt(), listOf(0xFF6E1E1C.toInt(), 0xFF4A1210.toInt(), 0xFF220807.toInt()), title = "REDLINE", year = "2023–", face = condensed, sunX = 0.55f, sunY = 0.45f),
        "white-valley" to Scene(0xFF2A5A80.toInt(), 0xFF0F2438.toInt(), 0xFFFFFFFF.toInt(), listOf(0xFF4F86AE.toInt(), 0xFF2B5677.toInt(), 0xFF112637.toInt()), title = "WHITE VALLEY", year = "2025–", face = serif, sunX = 0.62f),
        "ashfall" to Scene(0xFF5E1A16.toInt(), 0xFF1A0705.toInt(), 0xFFF4C9A0.toInt(), listOf(0xFF6A201A.toInt(), 0xFF451310.toInt(), 0xFF1C0705.toInt()), title = "ASHFALL", year = "2025", face = condensed, sunX = 0.75f, sunY = 0.55f),
        "lantern-house" to Scene(0xFF3E2A55.toInt(), 0xFF1A1226.toInt(), 0xFFF6D49A.toInt(), listOf(0xFF6A4A7E.toInt(), 0xFF46315A.toInt(), 0xFF22172E.toInt()), title = "The Lantern House", year = "2021–", face = serif, sunY = 0.42f, sunX = 0.6f),
        "saltwind" to Scene(0xFF9A4A22.toInt(), 0xFF3A1A0C.toInt(), 0xFFFBEFD0.toInt(), listOf(0xFFB0582A.toInt(), 0xFF7A3A1C.toInt(), 0xFF3A1A0C.toInt()), title = "SALTWIND", year = "2024", face = condensed, sunX = 0.68f, sunY = 0.3f),
        "parallel-coast" to Scene(0xFF2A5A6E.toInt(), 0xFF0E2630.toInt(), 0xFFF2E0B4.toInt(), listOf(0xFF2F6276.toInt(), 0xFF1E4252.toInt(), 0xFF0E2029.toInt()), title = "Parallel Coast", year = "2019–", face = serif, sunX = 0.8f, sunY = 0.3f),
        "driftline" to Scene(0xFF1C2A4A.toInt(), 0xFF0A1022.toInt(), 0xFFE6F0FF.toInt(), listOf(0xFF2A3C64.toInt(), 0xFF1A2846.toInt(), 0xFF0C1428.toInt()), stars = true, title = "DRIFTLINE", year = "2022–", face = heavy, sunX = 0.64f, sunY = 0.3f),
        "copper-sky" to Scene(0xFF7A4A20.toInt(), 0xFF2A160A.toInt(), 0xFFFFE2B0.toInt(), listOf(0xFF8A5428.toInt(), 0xFF5C3518.toInt(), 0xFF2A160A.toInt()), title = "Copper Sky", year = "2020", face = serif),
        "violet-hour" to Scene(0xFF2E1E4E.toInt(), 0xFF120A22.toInt(), 0xFFFFB8E0.toInt(), listOf(0xFF4A3070.toInt(), 0xFF2E1E48.toInt(), 0xFF140C22.toInt()), stars = true, title = "Violet Hour", year = "2021", face = heavy, sunY = 0.55f),
    )

    fun draw(name: String, width: Int, height: Int, poster: Boolean): Bitmap {
        val scene = all[name] ?: all.values.elementAt(Math.floorMod(name.hashCode(), all.size))
        val random = Random(name.hashCode())
        val bitmap = Bitmap.createBitmap(width.coerceAtLeast(1), height.coerceAtLeast(1), Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val w = width.toFloat()
        val h = height.toFloat()
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        paint.shader = LinearGradient(0f, 0f, 0f, h, scene.skyTop, scene.skyBottom, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null

        if (scene.stars) {
            paint.color = 0xCCFFFFFF.toInt()
            repeat(70) {
                canvas.drawCircle(random.nextFloat() * w, random.nextFloat() * h * 0.6f, random.nextFloat() * w * 0.004f + 0.5f, paint)
            }
        }

        val sunX = scene.sunX * w
        val sunY = scene.sunY * h
        val sunR = minOf(w, h) * if (poster) 0.075f else 0.06f
        paint.shader = RadialGradient(sunX, sunY, sunR * 4.5f, (scene.sun and 0x00FFFFFF) or 0x55000000, scene.sun and 0x00FFFFFF, Shader.TileMode.CLAMP)
        canvas.drawCircle(sunX, sunY, sunR * 4.5f, paint)
        paint.shader = null
        paint.color = scene.sun
        canvas.drawCircle(sunX, sunY, sunR, paint)

        val bases = listOf(0.55f, 0.66f, 0.78f)
        scene.hills.forEachIndexed { i, colour ->
            val path = Path()
            val base = bases[i] * h
            path.moveTo(0f, base)
            var x = 0f
            while (x < w) {
                val step = w * (0.06f + random.nextFloat() * 0.1f)
                x += step
                path.lineTo(x, base - random.nextFloat() * h * (0.12f - i * 0.03f))
            }
            path.lineTo(w, h)
            path.lineTo(0f, h)
            path.close()
            paint.color = colour
            canvas.drawPath(path, paint)
        }

        paint.shader = LinearGradient(0f, h * 0.55f, 0f, h, 0x00000000, 0xE0000000.toInt(), Shader.TileMode.CLAMP)
        canvas.drawRect(0f, h * 0.55f, w, h, paint)
        paint.shader = null

        if (poster && scene.title.isNotEmpty()) drawTitle(canvas, scene, w, h)
        return bitmap
    }

    private fun drawTitle(canvas: Canvas, scene: Scene, w: Float, h: Float) {
        val text = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            color = 0xFFF4F1EA.toInt()
            typeface = scene.face
            textSize = w * if (scene.title.length > 12) 0.13f else 0.16f
            setShadowLayer(w * 0.03f, 0f, w * 0.01f, 0x80000000.toInt())
        }
        val layout = StaticLayout.Builder.obtain(scene.title, 0, scene.title.length, text, (w * 0.9f).toInt())
            .setAlignment(Layout.Alignment.ALIGN_CENTER)
            .setLineSpacing(0f, 0.9f)
            .setMaxLines(2)
            .build()
        val top = h * 0.88f - layout.height - w * 0.06f
        canvas.save()
        canvas.translate(w * 0.05f, top)
        layout.draw(canvas)
        canvas.restore()
        val year = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            color = 0xB3F4F1EA.toInt()
            typeface = Typeface.create(Typeface.SANS_SERIF, Typeface.BOLD)
            textSize = w * 0.04f
            textAlign = Paint.Align.CENTER
            letterSpacing = 0.1f
        }
        canvas.drawText(scene.year, w / 2f, h * 0.92f, year)
    }
}
