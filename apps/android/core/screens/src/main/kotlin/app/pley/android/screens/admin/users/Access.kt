package app.pley.android.screens.admin.users

import app.pley.android.model.RequestAccess
import app.pley.android.model.ServerInfo
import app.pley.android.model.admin.UserAccount
import app.pley.android.screens.home.absolute

/**
 * What an account sees, or what an invitation's accounts will (the web's
 * AccessFields): every library, including those added later, or only
 * [libraryIds]; movies and shows up to [maxAge] (`null`: no limit).
 */
data class AccessChoice(
    val allLibraries: Boolean = true,
    val libraryIds: List<Long> = emptyList(),
    val maxAge: Int? = null,
) {
    /** "Only some" without one chosen, which the web asks to fix before sending. */
    val noLibrary: Boolean get() = !allLibraries && libraryIds.isEmpty()
}

/**
 * The age limits offered for accounts and invitations, as the web offers
 * them (apps/web/src/lib/access.ts AGE_OPTIONS, after the server's rating
 * table): no limit, then the oldest age each rating allows.
 */
val AGE_LIMITS: List<Int?> = listOf(null, 0, 7, 10, 13, 14, 15, 17)

/** [ids] with [id] added, or taken out when it was there. */
fun toggled(ids: List<Long>, id: Long): List<Long> = if (id in ids) ids - id else ids + id

/** What's wrong with a new password that can be told without asking the server (apps/web/src/lib/passwords.ts). */
enum class PasswordProblem { TooShort, SameAsUsername }

/** The shortest password the server takes. */
const val MIN_PASSWORD_LENGTH = 8

/** [password]'s problem for the account [username], or `null`; the server also refuses common passwords. */
fun passwordProblem(password: String, username: String = ""): PasswordProblem? = when {
    password.codePointCount(0, password.length) < MIN_PASSWORD_LENGTH -> PasswordProblem.TooShort
    username.isNotBlank() && password.lowercase() == username.trim().lowercase() -> PasswordProblem.SameAsUsername
    else -> null
}

private val PIN = Regex("^\\d{4,8}$")

/** A profile's PIN: 4 to 8 digits. */
fun pinOk(pin: String): Boolean = PIN.matches(pin)

/** Whether [user] has to set up two-step sign-in at their next sign-in, which [server] requires. */
fun mustSetUpTwoStep(user: UserAccount, server: ServerInfo?): Boolean =
    server?.twoStepRequired == true && user.hasPassword && !user.managed && !user.twoStep

/** An administrator's requests are approved as they make them; a managed profile is never one. */
val UserAccount.asAdmin: Boolean get() = isAdmin && !managed

/** What may [user] do with requests, as the page shows it: always approved for an administrator. */
fun requestsShown(user: UserAccount): RequestAccess? = if (user.asAdmin) RequestAccess.Auto else user.requestAccess

/**
 * What a new account may do with requests until it is chosen: as the
 * server decides for one made without it, a managed profile, or one with
 * an age limit, may not ask; others ask an administrator.
 */
fun requestsByDefault(managed: Boolean, maxAge: Int?): RequestAccess =
    if (managed || maxAge != null) RequestAccess.Off else RequestAccess.Ask

/** [user]'s picture's full address on [server]; `null` without one (the initial shows). */
fun avatarOf(user: UserAccount, server: String?): String? =
    user.avatarUrl?.takeIf { it.isNotEmpty() }?.let { absolute(it, server.orEmpty()) }
