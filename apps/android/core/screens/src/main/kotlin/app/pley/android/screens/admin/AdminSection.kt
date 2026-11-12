package app.pley.android.screens.admin

import kotlinx.serialization.Serializable

/**
 * The parts of managing the server a phone has, in the order of Settings in
 * the web app (apps/web/src/pages/settings/SettingsLayout.tsx), Dashboard
 * first. The server's settings, network, transcoder, requests and Live
 * TV stay in the web app: the settings and Live TV's sources hold its keys
 * and passwords, which the server keeps from apps.
 */
@Serializable
enum class AdminSection {
    Dashboard,
    Libraries,
    Users,
    Invitations,
}
