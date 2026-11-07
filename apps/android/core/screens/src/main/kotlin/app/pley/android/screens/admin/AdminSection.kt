package app.pley.android.screens.admin

import kotlinx.serialization.Serializable

/**
 * The parts of managing the server a phone has, in the order of Settings in
 * the web app (apps/web/src/pages/settings/SettingsLayout.tsx), Dashboard
 * first. The server's settings, network, transcoder, requests and Live
 * TV's sources hold its keys and passwords and stay in the web app, which
 * the server requires.
 */
@Serializable
enum class AdminSection {
    Dashboard,
    Libraries,
    LiveTv,
    Users,
    Invitations,
}
