package app.pley.android.designsystem.image

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp
import app.pley.android.model.ItemKind
import app.pley.android.model.TitleKind

/**
 * The glyph cut out of the front poster of Pley's mark, which a
 * placeholder draws once no picture will come: the kind of library its
 * title is in. packages/placeholder-mark/marks.json defines the mark for
 * every app; PlaceholderMarkTest holds this copy of it to that file.
 */
enum class MarkGlyph(internal val path: String) {
    /** The logo's play: extras, collections, playlists of video, anything not in a library of its own kind. */
    Play(
        "M19 27L19 37C19 37.36 19.19 37.69 19.51 37.87C19.82 38.05 20.2 38.04 20.51 37.86L29.01 32.86C29.31 32.68 29.5 " +
            "32.35 29.5 32C29.5 31.65 29.31 31.32 29.01 31.14L20.51 26.14C20.2 25.96 19.82 25.95 19.51 26.13C19.19 26.31 " +
            "19 26.64 19 27Z",
    ),

    /** A clapperboard. */
    Movies(
        "M17.3 30.51C17.36 30.69 17.56 30.79 17.74 30.73L20.8 29.74C20.93 29.7 21.03 29.57 21.04 29.43L21.17 27.18C21.19 " +
            "26.93 20.95 26.75 20.71 26.82L16.88 28.07C16.69 28.13 16.59 28.33 16.65 28.51ZM22 28.85C21.99 29.09 22.23 " +
            "29.28 22.46 29.2L24.69 28.47C24.83 28.43 24.93 28.31 24.94 28.16L25.07 25.91C25.09 25.66 24.85 25.48 24.61 " +
            "25.56L22.38 26.28C22.24 26.33 22.15 26.45 22.14 26.59ZM25.9 27.58C25.89 27.83 26.13 28.01 26.36 27.93L28.68 " +
            "27.18C28.86 27.12 28.96 26.92 28.9 26.74L28.25 24.74C28.19 24.56 28 24.46 27.81 24.52L26.28 25.02C26.14 25.06 " +
            "26.05 25.18 26.04 25.33ZM17.41 33.44C17.41 32.72 17.99 32.14 18.71 32.14L28.11 32.14C28.83 32.14 29.41 32.72 " +
            "29.41 33.44L29.41 38.24C29.41 38.96 28.83 39.54 28.11 39.54L18.71 39.54C17.99 39.54 17.41 38.96 17.41 38.24Z",
    ),

    /** A TV. */
    Shows(
        "M19.59 24.83C19.31 25.07 19.29 25.49 19.53 25.77L22.62 29.32C22.82 29.55 23.18 29.55 23.38 29.32L26.47 25.77C26.71 " +
            "25.49 26.69 25.07 26.41 24.83L26.39 24.82C26.11 24.58 25.7 24.6 25.46 24.88L23.23 27.44C23.11 27.58 22.89 " +
            "27.58 22.77 27.44L20.54 24.88C20.3 24.6 19.89 24.58 19.61 24.82ZM16.25 31.72C16.25 30.68 17.1 29.82 18.15 " +
            "29.82L27.85 29.82C28.9 29.82 29.75 30.68 29.75 31.72L29.75 37.52C29.75 38.57 28.9 39.42 27.85 39.42L18.15 " +
            "39.42C17.1 39.42 16.25 38.57 16.25 37.52Z",
    ),

    /** Notes. */
    Music(
        "M20.16 26.47L29.61 24.47L29.61 34.87C29.61 35.97 28.88 36.93 27.82 37.23C26.77 37.53 25.65 37.09 25.07 36.16C24.49 " +
            "35.23 24.61 34.02 25.35 33.22C26.09 32.41 27.28 32.19 28.26 32.68L28.26 27.26L21.51 28.69L21.51 36.87C21.51 " +
            "37.97 20.78 38.93 19.72 39.23C18.67 39.53 17.55 39.09 16.97 38.16C16.39 37.23 16.51 36.02 17.25 35.22C17.99 " +
            "34.41 19.18 34.19 20.16 34.68Z",
    ),

    /** An open book. */
    Audiobooks(
        "M16 27.58C16 27.01 16.52 26.58 17.08 26.7L21.95 27.69C22.18 27.74 22.35 27.95 22.35 28.18L22.35 36.86C22.35 37.18 " +
            "22.06 37.42 21.75 37.35L16.72 36.32C16.3 36.24 16 35.87 16 35.44ZM23.65 28.18C23.65 27.95 23.82 27.74 24.05 " +
            "27.69L28.92 26.7C29.48 26.58 30 27.01 30 27.58L30 35.44C30 35.87 29.7 36.24 29.28 36.32L24.25 37.35C23.94 " +
            "37.42 23.65 37.18 23.65 36.86Z",
    ),
    ;

    /**
     * The mark's posters back to front, this glyph cut out of the front
     * one: parsed the first time a placeholder draws it, then kept. The
     * two behind it are the same for every glyph.
     */
    internal val layers: Array<MarkLayer> by lazy {
        val (front, alpha) = PlaceholderMark.POSTERS.last()
        val cut = parse(front + path).apply { fillType = PathFillType.EvenOdd }
        arrayOf(*PlaceholderMark.behind, MarkLayer(cut, Color.White.copy(alpha = alpha)))
    }

    companion object {
        /** The glyph for an item of [kind] (the API's `kind`): its library's, or play. */
        fun of(kind: String?): MarkGlyph = KINDS[kind] ?: DEFAULT

        /** The glyph for an item of [kind]: its library's, or play. */
        fun of(kind: ItemKind): MarkGlyph = of(kind.wire)

        /** The glyph for a title of Discover or a request: a movie's or a show's. */
        fun of(kind: TitleKind): MarkGlyph = of(kind.wire)

        /** An item's kind to its glyph; any other kind takes [DEFAULT]. */
        internal val KINDS: Map<String, MarkGlyph> = mapOf(
            "movie" to Movies,
            "show" to Shows,
            "season" to Shows,
            "episode" to Shows,
            "artist" to Music,
            "album" to Music,
            "track" to Music,
            "audiobook" to Audiobooks,
            "part" to Audiobooks,
        )

        internal val DEFAULT = Play
    }
}

/** One of the mark's posters on its grid, and the white it is filled with. */
internal class MarkLayer(val path: Path, val color: Color)

/** Pley's mark as placeholders draw it: its posters and how big it is drawn. */
internal object PlaceholderMark {
    /** The side of the square the paths are on. */
    const val GRID = 64f

    /** How much of a placeholder's shorter side the mark takes, alone and over a title. */
    const val SHARE = 0.4f
    const val SHARE_OVER_TITLE = 0.26f

    /** The mark's largest side (a backdrop), and its gap to the title under it. */
    val Max = 120.dp
    val Gap = 10.dp

    /** The posters, back to front: each one's path, filled with white at its alpha. */
    val POSTERS: List<Pair<String, Float>> = listOf(
        "M46 21.5L49 21.5C50.66 21.5 52 22.84 52 24.5L52 39.5C52 41.16 50.66 42.5 49 42.5L46 42.5Z" to 0.16f,
        "M36 18.5L40.5 18.5C42.43 18.5 44 20.07 44 22L44 42C44 43.93 42.43 45.5 40.5 45.5L36 45.5Z" to 0.24f,
        "M16 15.5L30 15.5C32.21 15.5 34 17.29 34 19.5L34 44.5C34 46.71 32.21 48.5 30 48.5L16 48.5C13.79 48.5 12 46.71 12 " +
            "44.5L12 19.5C12 17.29 13.79 15.5 16 15.5Z" to 0.36f,
    )

    /** The posters behind the front one, parsed once for every glyph. */
    val behind: Array<MarkLayer> by lazy {
        POSTERS.dropLast(1).map { (path, alpha) -> MarkLayer(parse(path), Color.White.copy(alpha = alpha)) }.toTypedArray()
    }
}

private fun parse(path: String): Path = PathParser().parsePathString(path).toPath()
