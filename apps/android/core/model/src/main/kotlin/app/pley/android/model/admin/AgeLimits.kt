package app.pley.android.model.admin

import kotlinx.serialization.Serializable

/**
 * The rating systems an age limit is picked and shown in: GET
 * /api/ratings/systems (apps/server/src/api/accounts.rs). [region] is the
 * metadata region ("GB"), whose ratings are a limit's when it names none.
 */
@Serializable
data class RatingSystems(
    val region: String? = null,
    val systems: List<RatingSystem> = emptyList(),
)

/** A country's ratings, youngest first; [name] is the server's, in English. */
@Serializable
data class RatingSystem(
    val country: String,
    val name: String,
    val ratings: List<SystemRating> = emptyList(),
)

/** A rating and the age it sets; [kind] "film" or "tv" when the name means another age for the other. */
@Serializable
data class SystemRating(
    val name: String,
    val age: Int,
    val kind: String? = null,
)

/**
 * A label on titles or in profiles' rules: GET /api/labels, with how many
 * titles carry it ([items]) and how many rules name it ([rules]).
 */
@Serializable
data class LabelUse(
    val label: String,
    val items: Int = 0,
    val rules: Int = 0,
)

/**
 * What an app may know of the server's metadata keys: GET /api/system's
 * `tmdb_key`, whether one is set; `null` from a server before it.
 */
@Serializable
data class MetadataKeys(val tmdbKey: Boolean? = null)
