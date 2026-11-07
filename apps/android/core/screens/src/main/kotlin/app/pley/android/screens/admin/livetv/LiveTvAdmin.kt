package app.pley.android.screens.admin.livetv

import app.pley.android.model.admin.LineupGroup
import app.pley.android.screens.PleyKey

/**
 * The pages of Live TV's administration, as the web's tabs
 * (apps/web/src/pages/settings/LiveTvSettings.tsx): its first page (what
 * is being watched), the groups and the channels. Its options and sources
 * stay in the web app. [id] is what [PleyKey.Manage.id] carries.
 */
sealed interface LiveTvAdminPage {
    val id: String?

    data object Front : LiveTvAdminPage {
        override val id: String? = null
    }

    data object Groups : LiveTvAdminPage {
        override val id: String = "groups"
    }

    data object Channels : LiveTvAdminPage {
        override val id: String = "channels"
    }

    companion object {
        /** The page [id] names; the first page for what names none. */
        fun of(id: String?): LiveTvAdminPage = when {
            id == null -> Front
            id == Groups.id -> Groups
            id == Channels.id -> Channels
            else -> Front
        }
    }
}

/** A run of the groups list: a region's groups together, or a group of no region (or a region of one) alone. */
data class GroupSection(val region: String?, val groups: List<LineupGroup>) {
    val channels: Int get() = groups.sumOf { it.count }
    val allHidden: Boolean get() = groups.all { it.hidden == it.count }
    val allShown: Boolean get() = groups.all { it.hidden == 0 }
}

/** Which groups the Groups page lists. */
enum class GroupsShown { All, Shown, Hidden }

/**
 * The groups [query] and [shown] keep, by region in the lineup's order, as
 * the web's Groups lists them: a region's groups at its first group's place.
 */
fun groupSections(groups: List<LineupGroup>, query: String = "", shown: GroupsShown = GroupsShown.All): List<GroupSection> {
    val q = query.trim().lowercase()
    val kept = groups.filter { g ->
        (q.isEmpty() || g.name.lowercase().contains(q)) &&
            when (shown) {
                GroupsShown.All -> true
                GroupsShown.Hidden -> g.hidden == g.count
                GroupsShown.Shown -> g.hidden < g.count
            }
    }
    val byRegion = kept.filter { it.region != null }.groupBy { it.region!! }
    val placed = mutableSetOf<String>()
    val out = mutableListOf<GroupSection>()
    for (g in kept) {
        val region = g.region?.takeIf { (byRegion[it]?.size ?: 0) > 1 }
        when {
            region == null -> out += GroupSection(null, listOf(g))
            placed.add(region) -> out += GroupSection(region, byRegion.getValue(region))
        }
    }
    return out
}

/** The codecs a tuner's plan names, as people know them; others as the transcoder names them. */
fun codecName(codec: String?): String = when (codec) {
    "h264" -> "H.264"
    "hevc" -> "HEVC"
    "mpeg2video" -> "MPEG-2"
    "aac" -> "AAC"
    "ac3" -> "Dolby Digital"
    "eac3" -> "Dolby Digital Plus"
    "mp2" -> "MP2"
    "mp3" -> "MP3"
    else -> codec.orEmpty()
}

/** The hardware an encoder runs on; `null` for libx264, the processor, which is said in the language shown. */
fun encoderName(encoder: String): String? = when (encoder) {
    "libx264" -> null
    "h264_nvenc" -> "NVIDIA"
    "h264_qsv" -> "Intel Quick Sync"
    "h264_amf" -> "AMD"
    "h264_videotoolbox" -> "VideoToolbox"
    else -> encoder
}
