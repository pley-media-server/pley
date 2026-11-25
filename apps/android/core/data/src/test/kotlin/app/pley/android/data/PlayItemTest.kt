package app.pley.android.data

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.MaybeItem
import app.pley.android.model.UserState
import app.pley.android.network.OfflineException
import app.pley.android.testing.Fixtures
import java.io.IOException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PlayItemTest {
    @get:Rule
    val folder = TemporaryFolder()

    private fun item(id: Long, kind: ItemKind, position: Long = 0, watched: Boolean = false, parent: Long? = null) =
        ItemSummary(id = id, libraryId = 1, kind = kind, title = "Item $id", parentId = parent, userState = UserState(positionMs = position, watched = watched))

    @Test
    fun `movies, episodes and extras resume unless watched or from the start`() = runTest {
        val play = signedIn(folder.newFolder()).graph.playItem
        assertEquals(PlayTarget.Video(1, 60_000), play.resolve(item(1, ItemKind.Movie, 60_000), fromStart = false))
        assertEquals(PlayTarget.Video(1, 0), play.resolve(item(1, ItemKind.Movie, 60_000), fromStart = true))
        assertEquals(PlayTarget.Video(1, 0), play.resolve(item(1, ItemKind.Movie, 60_000, watched = true), fromStart = false))
        assertEquals(PlayTarget.Video(2, 5_000), play.resolve(item(2, ItemKind.Episode, 5_000), fromStart = false))
        assertEquals(PlayTarget.Video(3, 7_000), play.resolve(item(3, ItemKind.Extra, 7_000), fromStart = false))
    }

    @Test
    fun `a show plays its on-deck episode, from where it was`() = runTest {
        val data = signedIn(folder.newFolder())
        val onDeck: MaybeItem = Fixtures.decode("items/on_deck.json")
        val episode = onDeck.item!!

        val target = data.graph.playItem.resolve(item(201, ItemKind.Show), fromStart = false)
        assertEquals(PlayTarget.Video(episode.id, episode.userState.positionMs), target)
        assertEquals(listOf<Any?>(201L), data.api.calls("onDeck").single().args)

        assertEquals(PlayTarget.Video(episode.id, 0), data.graph.playItem.resolve(item(202, ItemKind.Season), fromStart = true))
    }

    @Test
    fun `a show with nothing to play says so`() = runTest {
        val data = signedIn(folder.newFolder()) { api.answer("onDeck", MaybeItem(null)) }
        assertEquals(PlayTarget.NothingToPlay, data.graph.playItem.resolve(item(201, ItemKind.Show), fromStart = false))
    }

    @Test
    fun `offline, a show plays the on-deck episode held`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.items.onDeck(201))
        testScheduler.runCurrent()
        data.api.fail("onDeck", OfflineException(IOException("down")))

        val target = data.graph.playItem.resolve(item(201, ItemKind.Show), fromStart = false)
        assertEquals(Fixtures.decode<MaybeItem>("items/on_deck.json").item!!.id, (target as PlayTarget.Video).itemId)
    }

    @Test
    fun `an album plays its tracks, a track its album from there`() = runTest {
        val tracks = (1L..4L).map { item(400 + it, ItemKind.Track, parent = 400) }
        val data = signedIn(folder.newFolder()) { api.answer("children", ItemList(tracks)) }

        assertEquals(PlayTarget.Music(tracks.map { it.id }, 0, tracks), data.graph.playItem.resolve(item(400, ItemKind.Album), false))
        assertEquals(PlayTarget.Music(tracks.map { it.id }, 2, tracks), data.graph.playItem.resolve(tracks[2], false))
    }

    @Test
    fun `an artist plays every album's tracks`() = runTest {
        val data = signedIn(folder.newFolder()) { api.answer("children", ItemList(listOf(item(9, ItemKind.Track)))) }
        // Every album answers the same track here: the artist's two "albums" give two.
        val target = data.graph.playItem.resolve(item(300, ItemKind.Artist), false) as PlayTarget.Music
        assertEquals(1, data.api.calls("children").count { it.args.single() == 300L })
        assertEquals(listOf(9L), target.itemIds.distinct())
    }

    @Test
    fun `a book goes on from its on-deck part where it was left, a part picked from it`() = runTest {
        val parts = listOf(item(71, ItemKind.Part, watched = true, parent = 70), item(72, ItemKind.Part, 600_000, parent = 70), item(73, ItemKind.Part, parent = 70))
        val data = signedIn(folder.newFolder()) {
            api.answer("children", ItemList(parts))
            api.answer("onDeck", MaybeItem(parts[1]))
        }
        val ids = parts.map { it.id }
        assertEquals(PlayTarget.Music(ids, 1, parts, 600_000), data.graph.playItem.resolve(item(70, ItemKind.Audiobook), false))
        assertEquals(PlayTarget.Music(ids, 0, parts, 0), data.graph.playItem.resolve(item(70, ItemKind.Audiobook), fromStart = true))
        // A part picked: the book from it, where it was left in it; one heard to its end from its start.
        assertEquals(PlayTarget.Music(ids, 1, parts, 600_000), data.graph.playItem.resolve(parts[1], false))
        assertEquals(PlayTarget.Music(ids, 0, parts, 0), data.graph.playItem.resolve(parts[0], false))

        // Where it goes on can't be read: the first part not heard to its end.
        data.api.fail("onDeck", OfflineException(IOException("down")))
        assertEquals(PlayTarget.Music(ids, 1, parts, 600_000), data.graph.playItem.resolve(item(70, ItemKind.Audiobook), false))
    }

    @Test
    fun `an empty album has nothing to play, a track alone plays alone`() = runTest {
        val data = signedIn(folder.newFolder()) { api.answer("children", ItemList(emptyList())) }
        assertEquals(PlayTarget.NothingToPlay, data.graph.playItem.resolve(item(400, ItemKind.Album), false))
        val lone = item(7, ItemKind.Track)
        assertEquals(PlayTarget.Music(listOf(7L), 0, listOf(lone)), data.graph.playItem.resolve(lone, false))
    }
}
