package app.pley.android.model

/**
 * Live TV's names as Pley shows them: a provider's "UK| Sport" as a region
 * and the rest. The phone's and the TV's lists and players show them so,
 * and so does the car's Live TV.
 */
object LiveNames {
    /**
     * A name's region and the rest, when it begins with one the way
     * providers write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as the
     * server splits group names.
     */
    fun splitRegion(name: String): Pair<String, String>? {
        val n = name.trim().replace(LEADING_BARS, "")
        val m = BRACKETED.matchEntire(n) ?: BARRED.matchEntire(n) ?: COLON.matchEntire(n) ?: return null
        val region = m.groupValues[1].trim()
        val rest = m.groupValues[2].replace(LEADING_MARKS, "").trim()
        return if (region.isNotEmpty() && rest.isNotEmpty()) region to rest else null
    }

    /**
     * A channel's name as a list shows it: in a group, without the region
     * the group already says ("Sky Sports 1" in UK · Sport, not "UK| Sky
     * Sports 1").
     */
    fun channelName(channel: LiveChannel, category: LiveCategory?): String {
        if (category !is LiveCategory.Group) return channel.name
        val name = splitRegion(channel.name) ?: return channel.name
        val group = splitRegion(channel.group) ?: return channel.name
        return if (name.first.equals(group.first, ignoreCase = true)) name.second else channel.name
    }

    /** "UK · Sport" for a group of a region; its name otherwise. */
    fun groupName(group: LiveGroup?, name: String): String =
        if (group?.region != null) "${group.region} · ${group.label}" else group?.label ?: name

    private val LEADING_BARS = Regex("^\\|+\\s*")
    private val BRACKETED = Regex("^\\[([^\\]]{1,16})]\\s*(.+)$")
    private val BARRED = Regex("^([^|]{1,16})\\|\\s*(.+)$")
    private val COLON = Regex("^([A-Z]{1,4}):\\s*(.+)$")
    private val LEADING_MARKS = Regex("^[|:\\-\\s]+")
}
