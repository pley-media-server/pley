package app.pley.android.common

/**
 * Tags on what tests and benchmark journeys look for on screen (Compose
 * testTag, exposed as resource ids to UI Automator: both interfaces' roots
 * set testTagsAsResourceId). Shared by both interfaces, so one journey
 * drives the phone and the TV.
 */
object TestTags {
    const val PHONE_ROOT = "pley.phone.root"
    const val TV_ROOT = "pley.tv.root"
    const val TV_RAIL = "pley.tv.rail"
    const val HOME_ROWS = "pley.home.rows"
    const val LIBRARY_GRID = "pley.library.grid"
    const val ITEM_PLAY = "pley.item.play"
    const val PLAYER_SURFACE = "pley.player.surface"
    const val PLACEHOLDER = "pley.placeholder"
}
