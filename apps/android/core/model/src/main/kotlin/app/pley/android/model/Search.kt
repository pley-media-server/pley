package app.pley.android.model

import kotlinx.serialization.Serializable

/**
 * GET /api/search (apps/server/src/api/browse.rs, `SearchResults`): items
 * ranked by relevance, and people whose name has words starting with each
 * word asked for, most credited first; and Live TV channels by name,
 * number or what's on, for those Live TV is open to. Up to the `limit`
 * asked for of each.
 */
@Serializable
data class SearchResults(
    val items: List<ItemSummary> = emptyList(),
    val people: List<PersonSummary> = emptyList(),
    val channels: List<LiveChannel> = emptyList(),
)
