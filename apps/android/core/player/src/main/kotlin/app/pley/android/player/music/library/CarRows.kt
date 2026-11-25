package app.pley.android.player.music.library

import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata

/**
 * This row as a browser of the older protocol (MediaBrowserCompat, as
 * Android Auto browses) is sent it: what the row shows, and nothing more.
 *
 * Media3 sends such a browser a list in one parcel and cuts it where it
 * passes 256 KiB (MediaLibraryServiceLegacyStub), the rows after the cut
 * never shown, and from a whole item it sends more than the row shows:
 * the item's media type, and the title again whenever a title to show is
 * set, each under a long key of Media3's own, some 250 bytes a row. Without
 * a title to show, Media3 makes the row's two lines of the title and the
 * artist, so the line under the title goes as the artist.
 */
@Suppress("DEPRECATION")
internal fun MediaItem.forOlderBrowser(): MediaItem {
    val m = mediaMetadata
    val row = MediaMetadata.Builder()
        .setTitle(m.displayTitle ?: m.title)
        .setArtist(m.subtitle)
        .setArtworkUri(m.artworkUri)
        // The kind of folder, which a car's Bluetooth browsing reads, as the media type made it: Android's own key.
        .setFolderType(m.folderType)
        .setIsBrowsable(m.isBrowsable)
        .setIsPlayable(m.isPlayable)
        .setExtras(m.extras)
        .build()
    return MediaItem.Builder().setMediaId(mediaId).setMediaMetadata(row).build()
}
