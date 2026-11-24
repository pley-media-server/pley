package androidx.media3.session

import android.os.Parcel
import androidx.media3.common.MediaItem

/**
 * The bytes [items] take in the parcel Android Auto is sent them in: Media3
 * turns each into a MediaBrowserCompat item ([LegacyConversions], which is
 * Media3's own, hence this package) and cuts the list where the parcel
 * passes 256 KiB (MediaLibraryServiceLegacyStub, MediaUtils.truncateListBySize).
 */
fun parcelledForCar(items: List<MediaItem>): Int {
    val parcel = Parcel.obtain()
    try {
        for (item in items) parcel.writeParcelable(LegacyConversions.convertToBrowserItem(item, null), 0)
        return parcel.dataSize()
    } finally {
        parcel.recycle()
    }
}

/** How many of [items] Media3 sends such a car: those before the parcel passes [CAR_LIST_BYTES], as it counts them. */
fun keptForCar(items: List<MediaItem>): Int = MediaUtils.truncateListBySize(items.map { LegacyConversions.convertToBrowserItem(it, null) }, CAR_LIST_BYTES).size

/**
 * What a car reads of [item]'s row: its id, its lines, its picture, whether it opens or plays, and the hints Android
 * Auto reads (its keys are Android's, "android.media."; Media3's own are for Media3's browsers).
 */
fun shownToCar(item: MediaItem): List<Any?> {
    val row = LegacyConversions.convertToBrowserItem(item, null)
    val description = row.description
    val extras = description.extras
    val hints = extras?.keySet()?.filter { it.startsWith("android.media.") }?.sorted()?.associateWith { @Suppress("DEPRECATION") extras.get(it) }?.takeIf { it.isNotEmpty() }
    return listOf(
        description.mediaId,
        description.title?.toString(),
        description.subtitle?.toString(),
        description.description?.toString(),
        description.iconUri,
        row.flags,
        hints,
    )
}

/** Where Media3 cuts a list it sends a car through MediaBrowserCompat. */
const val CAR_LIST_BYTES = 262_144
