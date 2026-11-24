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

    /** A channel's name as a row shows it: its [title], and the [region] said beside what is on (`null`: not said). */
    data class ChannelLine(val title: String, val region: String?)

    /**
     * The rows of [channels] listed together in [category]'s list, as the
     * car shows them (the phone's and the TV's lists can take them up): a
     * name read at a glance, "ABC" rather than "US| ABC ᴴᴰ", the provider's
     * region under it. The region isn't said where the row's group says it
     * or every row has the same one; the tags providers write after a name
     * in superscript or small capitals ("ᴴᴰ", "ᴿᴬᵂ") are left out, a plain
     * "HD" kept, and the name keeps its case. Two rows that would then read
     * the same (a provider's HD and RAW feeds of one channel) keep their
     * names as the provider wrote them.
     */
    fun channelLines(channels: List<LiveChannel>, category: LiveCategory?): List<ChannelLine> {
        val split = channels.map { splitRegion(it.name) }
        val inGroup = (category as? LiveCategory.Group)?.let { splitRegion(it.name)?.first }
        val regions = split.map { it?.first?.lowercase() }
        val shared = if (channels.size > 1 && regions.all { it != null && it == regions[0] }) regions[0] else null
        val lines = channels.zip(split) { channel, parts ->
            val region = parts?.first?.takeUnless { it.equals(inGroup, ignoreCase = true) || it.lowercase() == shared }
            ChannelLine(withoutTags(parts?.second ?: channel.name.trim()), region)
        }
        val seen = lines.groupingBy { it.title.lowercase() to it.region?.lowercase() }.eachCount()
        return lines.mapIndexed { i, line ->
            if (seen.getValue(line.title.lowercase() to line.region?.lowercase()) > 1) ChannelLine(channels[i].name.trim(), null) else line
        }
    }

    /** The names a channel goes by: the provider's, without its region, and either without its tags ([channelLines]). */
    fun namesOf(channel: LiveChannel): Set<String> {
        val name = channel.name.trim()
        val rest = splitRegion(name)?.second ?: name
        return setOf(name, rest, withoutTags(name), withoutTags(rest))
    }

    /** [name] without the tags written after it in superscript or small capitals; the name itself when that is all it is. */
    fun withoutTags(name: String): String {
        // Read back from the end, once: a provider's name is the provider's, and a pattern of repeats can be made slow.
        val whole = name.trim()
        var end = whole.length
        while (end > 0 && (isTag(whole[end - 1]) || whole[end - 1].isWhitespace())) end--
        return whole.substring(0, end).trimEnd().ifEmpty { whole }
    }

    /**
     * A superscript or small capital letter, digit or sign: the modifier letters, the phonetic extensions' small
     * capitals and superscripts, superscript and subscript digits and signs, and the small capitals elsewhere.
     */
    private fun isTag(c: Char): Boolean =
        c in 'ᴀ'..'ᵪ' || c in 'ᶛ'..'ᶿ' || c in '⁰'..'₟' || c in 'ʰ'..'ʸ' ||
            c in 'ˠ'..'ˤ' || c in OTHER_TAGS

    private const val OTHER_TAGS = "²³¹ⱽꜰꜱɢɪɴʀʏʙʜʟ"

    private val LEADING_BARS = Regex("^\\|+\\s*")
    private val BRACKETED = Regex("^\\[([^\\]]{1,16})]\\s*(.+)$")
    private val BARRED = Regex("^([^|]{1,16})\\|\\s*(.+)$")
    private val COLON = Regex("^([A-Z]{1,4}):\\s*(.+)$")
    private val LEADING_MARKS = Regex("^[|:\\-\\s]+")
}
