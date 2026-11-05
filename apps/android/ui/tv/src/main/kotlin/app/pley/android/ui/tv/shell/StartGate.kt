package app.pley.android.ui.tv.shell

import app.pley.android.screens.PleyKey

/**
 * What the TV's first screen is chosen by, read from this device only:
 * never the network, so the first frame waits for nothing.
 */
data class StartFacts(
    /** The server in use (its origin); `null` when none was opened. */
    val server: String?,
    /** Whether the app holds tokens for [server], read without the Keystore. */
    val signedIn: Boolean,
    /** How many profiles the household had when last seen; 1 while unknown. */
    val profiles: Int = 1,
    /** Settings' "Always start as" profile; `null` when not set. */
    val alwaysStartAs: Long? = null,
    /** Whether the profile the app was left on has a PIN, as last heard; `null` when it never heard who that is. */
    val hasPin: Boolean? = null,
)

/**
 * The TV's start gate (tv-addendum §3.3): no server → Servers; no tokens →
 * Link; otherwise Home as the profile the app was left on, but never past
 * its PIN. In a household of several profiles, one with a PIN asks first:
 * Who's watching?, or its pad with "Always start as"; and so does one the
 * app doesn't know has none.
 */
fun startKey(facts: StartFacts): PleyKey = when {
    facts.server == null -> PleyKey.Servers
    !facts.signedIn -> PleyKey.Link(facts.server)
    facts.profiles <= 1 -> PleyKey.Home
    facts.hasPin == false -> PleyKey.Home
    facts.alwaysStartAs == null || facts.hasPin == null -> PleyKey.Profiles()
    else -> PleyKey.Profiles(pin = true)
}
