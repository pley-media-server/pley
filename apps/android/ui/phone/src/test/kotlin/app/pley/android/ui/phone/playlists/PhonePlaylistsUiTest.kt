package app.pley.android.ui.phone.playlists

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ApplicationProvider
import app.pley.android.data.Loadable
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaylistKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.library.SmartFilter
import app.pley.android.screens.playlists.AddEvent
import app.pley.android.screens.playlists.AddToPlaylistUiState
import app.pley.android.screens.playlists.PlaylistCard
import app.pley.android.screens.playlists.PlaylistPage
import app.pley.android.screens.playlists.PlaylistRow
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.PhoneTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The phone's playlists: the empty page, a playlist's page (its buttons,
 * read-only smart playlists, the entries' actions for screen readers),
 * Add to playlist and New playlist.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
class PhonePlaylistsUiTest {
    @get:Rule
    val compose = createComposeRule()

    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit

        override fun restart(key: PleyKey) = Unit
    }

    private val done = mutableListOf<String>()

    private fun item(id: Long, title: String) = ItemSummary(id = id, libraryId = 1, kind = ItemKind.Movie, title = title, year = 2024)

    private fun row(entry: Long, title: String, number: Int) =
        PlaylistRow(entry, item(entry * 10, title), number, title, "2024", "1h 40m", ArtworkTileModel(id = entry * 10, title = title))

    private fun page(smart: Boolean = false) = PlaylistPage(
        id = 1,
        title = "Road trip",
        video = true,
        smart = smart,
        meta = "3 items · 5h",
        notes = if (smart) listOf("Genre: Comedy") else emptyList(),
        artwork = emptyList(),
        origin = "http://pley",
        lead = null,
        rows = listOf(row(1, "Northern Lights", 1), row(2, "Low Orbit", 2), row(3, "Halcyon", 3)),
    )

    private fun showPage(page: PlaylistPage<ArtworkTileModel>) {
        val actions = PlaylistActions(
            play = { from, shuffle -> done += "play $from $shuffle" },
            open = { done += "open ${it.entryId}" },
            more = { done += "more ${it.entryId}" },
            move = { from, to -> done += "move $from $to" },
            remove = { done += "remove ${it.entryId}" },
            rename = { done += "rename $it"; true },
            delete = { done += "delete" },
            retry = {},
            back = {},
        )
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalPhoneNavigator provides navigator) {
                    PlaylistContent(Loadable.Ready(page, fresh = true), actions)
                }
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun noPlaylistsSaysHowToMakeOne() {
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalPhoneNavigator provides navigator) {
                    PlaylistsContent(Loadable.Ready(emptyList(), fresh = true), onRetry = {})
                }
            }
        }
        compose.onNodeWithText("No playlists yet").assertExists()
        compose.onNodeWithText("New playlist").performClick()
        assertEquals(listOf<PleyKey>(PleyKey.NewPlaylist()), opened)
    }

    @Test
    fun aPlaylistPlaysShufflesAndOpensItsEntries() {
        showPage(page())
        compose.onNodeWithText("Video playlist").assertExists()
        compose.onNodeWithText("3 items · 5h").assertExists()
        compose.onNodeWithText("Play").performClick()
        compose.onNodeWithText("Shuffle").performClick()
        compose.onNodeWithContentDescription("Play Low Orbit").performClick()
        compose.onNodeWithText("Halcyon").performClick()
        assertEquals(listOf("play 0 false", "play 0 true", "play 1 false", "open 3"), done)
    }

    @Test
    fun screenReadersMoveAndRemoveEntries() {
        showPage(page())
        val second = compose.onNodeWithTag(ENTRY_TAG + 2)
        second.custom("Move up")
        second.custom("Move down")
        second.custom("Remove from playlist")
        assertEquals(listOf("move 1 0", "move 1 2", "remove 2"), done)
        // The first entry can't go up.
        compose.onNodeWithTag(ENTRY_TAG + 1).assert(
            SemanticsMatcher("no Move up") { node ->
                node.config.getOrElseNullable(SemanticsActions.CustomActions) { null }.orEmpty().none { it.label == "Move up" }
            },
        )
    }

    /** Runs the node's screen reader action called [label]. */
    private fun SemanticsNodeInteraction.custom(label: String) {
        val actions = fetchSemanticsNode().config[SemanticsActions.CustomActions]
        compose.runOnIdle { actions.first { it.label == label }.action() }
    }

    @Test
    fun aSmartPlaylistIsReadOnly() {
        showPage(page(smart = true))
        compose.onNodeWithText("Smart").assertExists()
        compose.onNodeWithText("3 items · 5h · Genre: Comedy").assertExists()
        compose.onNodeWithContentDescription("Rename playlist").assertDoesNotExist()
        compose.onNodeWithContentDescription("Delete playlist").assertExists()
        compose.onNodeWithTag(HANDLE_TAG + 1).assertDoesNotExist()
    }

    @Test
    fun deletingAsksFirst() {
        showPage(page())
        compose.onNodeWithContentDescription("Delete playlist").performClick()
        compose.onNodeWithText("“Road trip” will be deleted. What's in it stays in your libraries.").assertExists()
        compose.onNodeWithText("Delete").performClick()
        assertEquals(listOf("delete"), done)
    }

    @Test
    fun addToPlaylistOffersANewOneAndTheOnesThatTakeIt() {
        val created = mutableListOf<String>()
        val added = mutableListOf<Long>()
        val card = PlaylistCard(4, "Favourites", "12 items · 19h 6m", video = true, smart = false, artwork = emptyList(), origin = "http://pley")
        compose.setContent {
            PhoneTheme {
                AddToPlaylistContent(
                    AddToPlaylistUiState("Heat (1995)", PlaylistKind.Video, Loadable.Ready(listOf(card), fresh = true), busy = false),
                    onCreate = { created += it; true },
                    onAdd = { added += it.id },
                )
            }
        }
        compose.onNodeWithText("Heat (1995)").assertExists()
        compose.onNodeWithText("Create").assertIsNotEnabled()
        compose.onNodeWithTag(ADD_NEW_FIELD).performTextInput("Weekend")
        compose.onNodeWithText("Create").assertIsEnabled().performClick()
        compose.onNodeWithText("Favourites").performClick()
        assertEquals(listOf("Weekend"), created)
        assertEquals(listOf(4L), added)
    }

    @Test
    fun theToastsSayHowManyWentIn() {
        val resources = ApplicationProvider.getApplicationContext<android.content.Context>().resources
        assertEquals("Added to Road trip", addedMessage(resources, AddEvent.Added("Road trip", 1)))
        assertEquals("Added to Road trip", addedMessage(resources, AddEvent.Added("Road trip", null)))
        assertEquals("Added 8 items to Road trip", addedMessage(resources, AddEvent.Added("Road trip", 8)))
        assertEquals("Nothing to add", addedMessage(resources, AddEvent.Added("Road trip", 0)))
    }

    @Test
    fun aNewPlaylistAsksWhatItPlays() {
        val made = mutableListOf<Pair<String, PlaylistKind>>()
        compose.setContent {
            PhoneTheme {
                NewPlaylistContent(smart = false, filters = emptyList(), asksKind = true, busy = false, onCreate = { t, k -> made += t to k; true }, onCancel = {})
            }
        }
        compose.onNodeWithTag(NEW_FIELD).performTextInput("Chill")
        compose.onNodeWithText("Music").performClick()
        compose.onNodeWithText("Create").performClick()
        assertEquals(listOf("Chill" to PlaylistKind.Audio), made)
    }

    @Test
    fun savingFiltersSaysWhatTheyFind() {
        compose.setContent {
            PhoneTheme {
                NewPlaylistContent(
                    smart = true,
                    filters = SmartFilter.words("genre=Comedy&unwatched=1"),
                    asksKind = false,
                    busy = false,
                    onCreate = { _, _ -> true },
                    onCancel = {},
                )
            }
        }
        compose.onNodeWithText("Save these filters").assertExists()
        compose.onNodeWithText("Genre: Comedy · Unwatched. It keeps up with the library by itself.").assertExists()
        compose.onNodeWithText("Music").assertDoesNotExist()
    }
}
