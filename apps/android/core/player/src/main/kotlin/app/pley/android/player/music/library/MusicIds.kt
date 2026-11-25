package app.pley.android.player.music.library

import app.pley.android.model.LiveCategory

/**
 * The ids of the music library's nodes, as car screens and voice
 * assistants name them back ([MusicLibrary]): short, readable, and parsed
 * strictly. A track carries where it was picked from, so playing it
 * plays on through its album or playlist.
 */
sealed interface MusicId {
    val text: String

    data object Root : MusicId {
        override val text = "root"
    }

    /** The albums played last, then those added last to each music library. */
    data object Recent : MusicId {
        override val text = "recent"
    }

    /** The artists, or with several music libraries, the libraries. */
    data object Artists : MusicId {
        override val text = "artists"
    }

    data object Playlists : MusicId {
        override val text = "playlists"
    }

    /** The artists of one music library, when there are several. */
    data class LibraryArtists(val libraryId: Long) : MusicId {
        override val text get() = "artists:$libraryId"
    }

    /** An artist's albums, with Shuffle above them. */
    data class Artist(val id: Long) : MusicId {
        override val text get() = "artist:$id"
    }

    /** All of an artist's tracks, shuffled. */
    data class ShuffleArtist(val id: Long) : MusicId {
        override val text get() = "shuffle:$id"
    }

    /** An album: its tracks, and played, all of them. */
    data class Album(val id: Long) : MusicId {
        override val text get() = "album:$id"
    }

    /** A music playlist: its tracks, and played, all of them. */
    data class Playlist(val id: Long) : MusicId {
        override val text get() = "playlist:$id"
    }

    /**
     * A track, picked from its [from] (an album or a playlist), or alone (a search: its album plays on). A book's
     * part is one too, in the queue: its book plays on from it.
     */
    data class Track(val id: Long, val from: MusicId? = null) : MusicId {
        override val text get() = if (from == null) "track:$id" else "track:$id@${from.text}"
    }

    /** The audiobooks' tab: those being heard, those added last, then every one by title. */
    data object Books : MusicId {
        override val text = "books"
    }

    /** An audiobook: played, it goes on where the profile is in it, its parts the queue. */
    data class Book(val id: Long) : MusicId {
        override val text get() = "book:$id"
    }

    /** Live TV's tab: favorites, the channels watched lately, and the groups. */
    data object LiveTv : MusicId {
        override val text = "live"
    }

    /** The channels of a Live TV [category]: one group's, or every one. */
    data class LiveList(val category: LiveCategory) : MusicId {
        override val text get() = "live:${category.key}"
    }

    /** A channel, picked from its [from] list: Next and Previous go through it. */
    data class Channel(val id: Long, val from: LiveCategory? = null) : MusicId {
        override val text get() = if (from == null) "channel:$id" else "channel:$id@${from.key}"
    }

    companion object {
        /** The id [text] names; `null` for anything else. */
        fun parse(text: String): MusicId? {
            if (text.length > MAX_LENGTH) return null
            when (text) {
                Root.text -> return Root
                Recent.text -> return Recent
                Artists.text -> return Artists
                Playlists.text -> return Playlists
                Books.text -> return Books
                LiveTv.text -> return LiveTv
            }
            val list = text.removePrefix("live:")
            if (list != text) return category(list)?.let { LiveList(it) }
            val channel = text.removePrefix("channel:")
            if (channel != text) {
                val at = channel.indexOf('@')
                if (at < 0) return channel.toIdOrNull()?.let { Channel(it) }
                val id = channel.substring(0, at).toIdOrNull() ?: return null
                return category(channel.substring(at + 1))?.let { Channel(id, it) }
            }
            val track = text.removePrefix("track:")
            if (track != text) {
                val at = track.indexOf('@')
                if (at < 0) return track.toIdOrNull()?.let { Track(it) }
                val id = track.substring(0, at).toIdOrNull() ?: return null
                val from = parse(track.substring(at + 1))
                return if (from is Album || from is Playlist) Track(id, from) else null
            }
            val colon = text.indexOf(':')
            if (colon < 0) return null
            val id = text.substring(colon + 1).toIdOrNull() ?: return null
            return when (text.substring(0, colon)) {
                "artists" -> LibraryArtists(id)
                "artist" -> Artist(id)
                "shuffle" -> ShuffleArtist(id)
                "album" -> Album(id)
                "playlist" -> Playlist(id)
                "book" -> Book(id)
                else -> null
            }
        }

        /** A category's key, strictly: "all", "favorites", "recent" or "group:" and a group's name. */
        private fun category(key: String): LiveCategory? = when {
            key == LiveCategory.All.key || key == LiveCategory.Favorites.key || key == LiveCategory.Recent.key -> LiveCategory.parse(key)
            key.startsWith("group:") && key.length > "group:".length -> LiveCategory.parse(key)
            else -> null
        }

        /** The longest id taken: a group's name is the provider's, and a car sends ids back. */
        private const val MAX_LENGTH = 512

        /** A positive id, digits only. */
        private fun String.toIdOrNull(): Long? =
            if (isEmpty() || length > 19 || any { it !in '0'..'9' }) null else toLongOrNull()?.takeIf { it > 0 }
    }
}
