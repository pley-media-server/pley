package app.pley.android.model.admin

import kotlinx.serialization.Serializable

/**
 * A link that makes an account: GET /api/invites, and what POST
 * /api/invites answers (apps/server/src/api/accounts.rs, `InviteDto`).
 * Its account sees [allLibraries] or [libraryIds], up to [maxAge]. Times
 * are unix seconds; [expiresAt] `null`: never.
 */
@Serializable
data class Invitation(
    val code: String,
    val createdAt: Long = 0,
    val expiresAt: Long? = null,
    val usesLeft: Int = 0,
    val allLibraries: Boolean = true,
    val libraryIds: List<Long> = emptyList(),
    val maxAge: Int? = null,
    /** What the administrator noted, for themselves ("for Sam"). */
    val note: String? = null,
)

/**
 * POST /api/invites: [expiresInDays] (0: never), how many accounts it
 * makes ([uses], 1 to 1000), and their access. A `null` is left out.
 */
@Serializable
data class NewInvitation(
    val expiresInDays: Int,
    val uses: Int,
    val allLibraries: Boolean = true,
    val libraryIds: List<Long> = emptyList(),
    val maxAge: Int? = null,
    val note: String? = null,
)
