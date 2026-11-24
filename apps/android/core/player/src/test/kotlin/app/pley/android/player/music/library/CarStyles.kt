package app.pley.android.player.music.library

import androidx.media3.common.MediaItem
import androidx.media3.session.MediaConstants

/**
 * How Android Auto shows [child], a row of [parent]'s list as the car was
 * sent [parent] (`null`: the root, or search): its own hint, else its
 * parent's for its kind (what opens, what plays), else the root's.
 */
internal fun shownAs(parent: MediaItem?, child: MediaItem): Int {
    child.mediaMetadata.extras?.takeIf { it.containsKey(SINGLE) }?.let { return it.getInt(SINGLE) }
    val opens = child.mediaMetadata.isBrowsable == true
    val key = if (opens) MediaConstants.EXTRAS_KEY_CONTENT_STYLE_BROWSABLE else MediaConstants.EXTRAS_KEY_CONTENT_STYLE_PLAYABLE
    parent?.mediaMetadata?.extras?.takeIf { it.containsKey(key) }?.let { return it.getInt(key) }
    return if (opens) MusicLibrary.ROOT_BROWSABLE else MusicLibrary.ROOT_PLAYABLE
}

/** The styles' names, for a test's message. */
internal fun styleName(style: Int): String = when (style) {
    MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM -> "list"
    MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_GRID_ITEM -> "grid"
    MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_CATEGORY_LIST_ITEM -> "category list"
    MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_CATEGORY_GRID_ITEM -> "category grid"
    else -> "style $style"
}

private const val SINGLE = MediaConstants.EXTRAS_KEY_CONTENT_STYLE_SINGLE_ITEM
