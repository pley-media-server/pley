package app.pley.android.screens

import app.pley.android.model.TitleKind
import kotlinx.serialization.descriptors.elementNames
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PleyKeyTest {
    private val every: List<PleyKey> = listOf(
        PleyKey.Servers,
        PleyKey.SignIn("http://192.168.1.20:7539"),
        PleyKey.Link("http://192.168.1.20:7539"),
        PleyKey.Profiles,
        PleyKey.Home,
        PleyKey.Libraries,
        PleyKey.Library(3, LibraryTab.Recommended),
        PleyKey.Collections(3),
        PleyKey.Collection(12),
        PleyKey.Item(4567),
        PleyKey.Person(89),
        PleyKey.Artist(700),
        PleyKey.Album(701),
        PleyKey.Search("blade runner"),
        PleyKey.Playlists,
        PleyKey.Playlist(5),
        PleyKey.LiveTv,
        PleyKey.Discover,
        PleyKey.DiscoverResults("arrival"),
        PleyKey.DiscoverTitle(TitleKind.Show, 1396, pick = true),
        PleyKey.Requests,
        PleyKey.Me,
        PleyKey.History,
        PleyKey.Downloads,
        PleyKey.Settings,
        PleyKey.Diagnostics,
        PleyKey.NowPlaying,
        PleyKey.ItemActions(4567),
        PleyKey.ItemActions(204, continueWatching = true),
        PleyKey.AddToPlaylist(listOf(1, 2, 3)),
        PleyKey.NewPlaylist(listOf(4567)),
        PleyKey.SaveSmartPlaylist(3, "sort=year&order=desc&unwatched=1"),
    )

    @Test
    fun everyPlaceSurvivesBeingSaved() {
        // The back stack is saved as these keys (process death, rotation).
        for (key in every) {
            val text = Json.encodeToString(PleyKey.serializer(), key)
            assertEquals(text, key, Json.decodeFromString(PleyKey.serializer(), text))
        }
    }

    @Test
    fun everyKindOfPlaceIsCovered() {
        // The sealed serializer lists every kind of key, by class name.
        val kinds = PleyKey.serializer().descriptor.getElementDescriptor(1).elementNames.toSet()
        assertEquals(kinds, every.map { it::class.qualifiedName }.toSet())
    }

    @Test
    fun sheetsOpenOverTheirScreen() {
        assertTrue(PleyKey.ItemActions(1).isSheet)
        assertTrue(PleyKey.AddToPlaylist(listOf(1)).isSheet)
        assertFalse(PleyKey.Item(1).isSheet)
        assertFalse(PleyKey.Home.isSheet)
    }
}
