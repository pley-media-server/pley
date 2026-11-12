package app.pley.android.screens.admin.users

import app.pley.android.model.RequestAccess
import app.pley.android.model.ServerInfo
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.UserAccount
import app.pley.android.screens.home.absolute
import java.time.LocalDate
import java.time.YearMonth
import java.util.Locale

/**
 * What an account sees, or what an invitation's accounts will (the web's
 * AccessFields): every library, including those added later, or only
 * [libraryIds]; movies and shows up to [maxAge] (`null`: no limit), picked
 * and shown in [maxAgeSystem] (a country, or [AGE_SYSTEM]; `null`: the
 * metadata region's), or following the age of someone born in
 * [birthMonth] ("2015-03", accounts only); and the exceptions by label,
 * titles always shown ([showLabels]) or never ([hideLabels]).
 */
data class AccessChoice(
    val allLibraries: Boolean = true,
    val libraryIds: List<Long> = emptyList(),
    val maxAge: Int? = null,
    val maxAgeSystem: String? = null,
    val birthMonth: String? = null,
    val showLabels: List<String> = emptyList(),
    val hideLabels: List<String> = emptyList(),
) {
    /** "Only some" without one chosen, which the web asks to fix before sending. */
    val noLibrary: Boolean get() = !allLibraries && libraryIds.isEmpty()

    /** Labels both always and never shown, which the server refuses. */
    val labelsInBoth: List<String> get() = inBoth(showLabels, hideLabels)
}

/** [user]'s access, as its page shows and changes it. */
fun accessOf(user: UserAccount): AccessChoice = AccessChoice(
    allLibraries = user.allLibraries,
    libraryIds = user.libraryIds,
    maxAge = user.maxAge,
    maxAgeSystem = user.maxAgeSystem,
    birthMonth = user.birthMonth,
    showLabels = user.showLabels,
    hideLabels = user.hideLabels,
)

/** The rating system of plain ages, 0 to 21. */
const val AGE_SYSTEM = "age"

/** The age a limit that follows a birthday ends at, as the server's; no rating is for anyone older. */
const val BIRTHDAY_LIMIT_ENDS = 21

/** The ages a limit can be set to. */
val AGES: List<Int> = (0..BIRTHDAY_LIMIT_ENDS).toList()

/** One choice of a system: an age and the ratings for it ("PG, TV-PG"); [rating] `null` in [AGE_SYSTEM]. */
data class AgeLevel(val age: Int, val rating: String?)

/** How a rating's name says which of films and TV it is for, when its name is at two ages. */
typealias RatingFor = (name: String, kind: String) -> String

private val plainRatingFor: RatingFor = { name, kind -> "$name ($kind)" }

/**
 * The ages [system]'s ratings stand for, youngest first, with their names
 * (lib/access.ts ageLevels). A name that's at two ages (Canada's PG: 10 for
 * a film, 8 on TV) says which, in [ratingFor]'s words ("PG (film)").
 */
fun ageLevels(systems: RatingSystems?, system: String, ratingFor: RatingFor = plainRatingFor): List<AgeLevel> {
    val found = systems?.systems?.firstOrNull { it.country == system } ?: return AGES.map { AgeLevel(it, null) }
    val agesOf = found.ratings.groupBy { it.name }.mapValues { (_, ratings) -> ratings.map { it.age }.toSet() }
    val byAge = linkedMapOf<Int, MutableList<String>>()
    for (rating in found.ratings) {
        val kind = rating.kind
        val name = if (kind != null && agesOf[rating.name].orEmpty().size > 1) ratingFor(rating.name, kind) else rating.name
        val names = byAge.getOrPut(rating.age) { mutableListOf() }
        if (name !in names) names += name
    }
    return byAge.entries.sortedBy { it.key }.map { (age, names) -> AgeLevel(age, names.joinToString(", ")) }
}

/** How an age limit is set: what the age limit row shows. */
enum class LimitMode { None, Rating, Age, Birthday }

fun limitMode(systems: RatingSystems?, access: AccessChoice): LimitMode = when {
    access.birthMonth != null -> LimitMode.Birthday
    access.maxAge == null -> LimitMode.None
    shownSystem(systems, access.maxAgeSystem, access.maxAge) == AGE_SYSTEM -> LimitMode.Age
    else -> LimitMode.Rating
}

/** The country to pick a rating in: the one stored, else the metadata region, else the US. */
fun ratingSystem(systems: RatingSystems?, stored: String?): String? {
    val countries = systems?.systems.orEmpty()
    fun has(code: String?) = code != null && countries.any { it.country == code }
    return when {
        has(stored) -> stored
        has(systems?.region) -> systems?.region
        // A region without ratings here reads ratings without a country as the US's, as the server does.
        has("US") -> "US"
        else -> countries.firstOrNull()?.country
    }
}

/**
 * The system a limit shows in: the one stored, else the metadata region's,
 * else [AGE_SYSTEM]; and [AGE_SYSTEM] when the system has no rating for the
 * limit (8 in the US), so that it never reads as no limit.
 */
fun shownSystem(systems: RatingSystems?, stored: String?, maxAge: Int?): String {
    fun has(code: String?) = code != null && (code == AGE_SYSTEM || systems?.systems.orEmpty().any { it.country == code })
    val region = systems?.region
    val system = when {
        stored != null && has(stored) -> stored
        region != null && has(region) -> region
        else -> AGE_SYSTEM
    }
    if (maxAge == null || system == AGE_SYSTEM) return system
    return if (ageLevels(systems, system).any { it.age == maxAge }) system else AGE_SYSTEM
}

/** The limit to keep when the system changes: the new system's highest at or below it, else its lowest. */
fun ageIn(systems: RatingSystems?, system: String, maxAge: Int?): Int? {
    if (maxAge == null) return null
    val ages = ageLevels(systems, system).map { it.age }
    return ages.lastOrNull { it <= maxAge } ?: ages.firstOrNull() ?: maxAge
}

/** The ratings a limit is shown as ("PG-13"); `null` in [AGE_SYSTEM]. */
fun limitRating(systems: RatingSystems?, stored: String?, maxAge: Int, ratingFor: RatingFor = plainRatingFor): String? =
    ageLevels(systems, shownSystem(systems, stored, maxAge), ratingFor).firstOrNull { it.age == maxAge }?.rating

/** "2015-03" → its month; `null` when it isn't one. */
fun parseBirthMonth(birthMonth: String): YearMonth? {
    val match = BIRTH_MONTH.matchEntire(birthMonth) ?: return null
    val month = match.groupValues[2].toInt()
    return if (month in 1..12) YearMonth.of(match.groupValues[1].toInt(), month) else null
}

private val BIRTH_MONTH = Regex("""(\d{4})-(\d{2})""")

fun birthMonthOf(year: Int, month: Int): String = "%04d-%02d".format(Locale.ROOT, year, month)

/**
 * The limit of someone born in [birthMonth] (their age, which goes up on
 * the first of the month they were born in), as the server works it out;
 * `null` from [BIRTHDAY_LIMIT_ENDS].
 */
fun birthdayLimit(birthMonth: String, today: LocalDate = LocalDate.now()): Int? {
    val born = parseBirthMonth(birthMonth) ?: return null
    val age = maxOf(0, today.year - born.year - if (today.monthValue < born.monthValue) 1 else 0)
    return age.takeIf { it < BIRTHDAY_LIMIT_ENDS }
}

/** The month a limit that follows [birthMonth] ends. */
fun birthdayLimitEnd(birthMonth: String): YearMonth? = parseBirthMonth(birthMonth)?.plusYears(BIRTHDAY_LIMIT_ENDS.toLong())

/** The years a birthday is picked from: this one and the [BIRTHDAY_LIMIT_ENDS] before, newest first, and [birthMonth]'s. */
fun birthYears(birthMonth: String?, today: LocalDate = LocalDate.now()): List<Int> {
    val years = (0..BIRTHDAY_LIMIT_ENDS).map { today.year - it }
    val born = birthMonth?.let(::parseBirthMonth)?.year
    return if (born == null || born in years) years else years + born
}

/**
 * The access with its age limit set [mode]'s way, kept as near as it can
 * be, and strict when there was none (the web's AccessFields setMode).
 * [birthday]: the limit can follow one (accounts, not invitations); a
 * limit set another way then stops following it.
 */
fun AccessChoice.withMode(mode: LimitMode, systems: RatingSystems?, birthday: Boolean, today: LocalDate = LocalDate.now()): AccessChoice {
    val fixed = if (birthday) copy(birthMonth = null) else this
    return when (mode) {
        LimitMode.None -> fixed.copy(maxAge = null)
        LimitMode.Age -> fixed.copy(maxAgeSystem = AGE_SYSTEM, maxAge = maxAge ?: 0)
        // Born this month, as many years ago as the limit was: the limit stays.
        LimitMode.Birthday -> withBirthMonth(birthMonthOf(today.year - (maxAge ?: 10), today.monthValue), today)
        LimitMode.Rating -> {
            val country = ratingSystem(systems, maxAgeSystem) ?: return this
            // The region's ratings stay "the region's" (null) unless another country is picked.
            val system = if (maxAgeSystem == null && country == systems?.region) null else country
            fixed.copy(maxAgeSystem = system, maxAge = ageIn(systems, country, maxAge ?: 0))
        }
    }
}

/** The limit picked in [system]'s ratings, kept as near as it can be. */
fun AccessChoice.withSystem(systems: RatingSystems?, system: String): AccessChoice =
    copy(maxAgeSystem = system, maxAge = ageIn(systems, system, maxAge))

/** A plain age, kept as one, so that one a rating also has (12 in the UK) doesn't turn into that rating. */
fun AccessChoice.withAge(age: Int): AccessChoice = copy(maxAgeSystem = AGE_SYSTEM, maxAge = age)

/** A limit that follows the age of someone born in [birthMonth]. */
fun AccessChoice.withBirthMonth(birthMonth: String, today: LocalDate = LocalDate.now()): AccessChoice =
    copy(birthMonth = birthMonth, maxAge = birthdayLimit(birthMonth, today))

/** The longest label the server keeps, in characters. */
const val MAX_LABEL = 120

/**
 * A label as the server matches it: the case of its letters A to Z
 * doesn't count, as in its database ("Kids" is "kids", "École" isn't
 * "école").
 */
fun labelKey(label: String): String = buildString(label.length) {
    for (c in label) append(if (c in 'A'..'Z') c + ('a' - 'A') else c)
}

/** [labels] with [text] added, trimmed: unchanged when it's empty or there already, as the server matches labels. */
fun addLabel(labels: List<String>, text: String): List<String> {
    val label = text.trim().take(MAX_LABEL)
    if (label.isEmpty() || labels.any { labelKey(it) == labelKey(label) }) return labels
    return labels + label
}

/** Labels in both lists, as the server matches them: always and never shown at once, which it refuses. */
fun inBoth(show: List<String>, hide: List<String>): List<String> = show.filter { s -> hide.any { labelKey(it) == labelKey(s) } }

/** Whether two lists hold the same labels, whatever their order. */
fun sameLabels(a: List<String>, b: List<String>): Boolean = a.size == b.size && a.all { it in b }

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
