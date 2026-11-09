package app.pley.android.screens.item

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.model.CollectionRef
import app.pley.android.model.Credit
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.screens.PleyKey

/**
 * What an item page shows: a movie, show, season or episode, as
 * apps/web/src/pages/item shows them. Everything is worked out off the
 * main thread; the phone and the TV only lay it out and put their words
 * on the [kinds][PrimaryAction.Kind] and labels.
 */
sealed interface ItemUiState {
    /** Nothing known yet: a skeleton. */
    data object Loading : ItemUiState

    /** No such item, or one this profile may not see (404). */
    data object NotFound : ItemUiState

    /** It didn't load and nothing was saved: [error]'s sentence and Retry. */
    data class Failed(val error: Throwable) : ItemUiState

    /**
     * The item has another page: artists, albums and tracks have the music
     * pages, and an extra shows its movie's or show's. The page swaps
     * itself for [key].
     */
    data class Elsewhere(val key: PleyKey) : ItemUiState

    /** The page, with how fresh it is ([status]: what was saved, the server's answer, or saved because the server failed). */
    data class Ready(val page: ItemPage, val status: Loadable<Unit>) : ItemUiState
}

/** An item's page, as both interfaces show it. */
data class ItemPage(
    val id: Long,
    /** Movie, show, season or episode. */
    val kind: ItemKind,
    /** The server's origin: pictures are made absolute against it. */
    val origin: String,
    /** The item as listings carry it: its pictures, and what its actions act on. */
    val item: ItemSummary,
    /** The item's own title; a season's is "Season 2" or "Specials". */
    val title: String,
    /** Episodes: their show and season. Seasons: their show. */
    val context: ItemContext?,
    val meta: List<MetaPart>,
    val genres: List<String>,
    /** The collections it is "Part of". */
    val partOf: List<CollectionRef>,
    val primary: PrimaryAction,
    /** "From start" plays from the beginning what Play would resume. */
    val fromStart: Boolean,
    /** Watched: movies and episodes, or every episode of a show or season. */
    val watched: Boolean,
    /** In My List; `null` where My List doesn't take the item (seasons, episodes). */
    val myList: Boolean?,
    /** The profile's own rating, 0 to 10 (a half star each); `null`: unrated. Movies, shows and episodes. */
    val userRating: Int?,
    val rateable: Boolean,
    val tagline: String?,
    val summary: String?,
    /** The version, audio and subtitle choices; `null` when there is nothing to choose. */
    val tracks: TrackChoices?,
    /** The first trailer among the extras. */
    val trailer: ItemSummary?,
    val details: ItemDetails,
    /** The files, as the collapsed "Media info" lists them. */
    val mediaInfo: List<FileInfo>,
    val extras: List<ItemSummary>,
    /** "Cast", or "Guest Stars" for an episode's. */
    val castTitle: CastTitle,
    /** Each person once, in billing order. */
    val cast: List<Credit>,
    /** More Like This: movies and shows only. */
    val related: List<ItemSummary>,
    /** Shows, seasons and episodes: the seasons and the episodes of one. */
    val seasons: SeasonsSection?,
)

/** Where an episode or season sits: the breadcrumb, and the TV's context line. */
data class ItemContext(
    val showId: Long?,
    val showTitle: String?,
    /** An episode's season. */
    val seasonId: Long?,
    /** "Season 2", "Specials". */
    val seasonTitle: String?,
    /** An episode's "S2 · E5". */
    val episodeLabel: String?,
    /** An episode's "Season 2 · Episode 5". */
    val episodeLongLabel: String?,
)

/** One piece of the metadata line, in order; the interfaces put "·" between them. */
sealed interface MetaPart {
    /** A year, an air date, "3 seasons", "2021-2025", an edition, a runtime. */
    data class Text(val text: UiText) : MetaPart {
        /** Words that are no language's own: a year, an edition, a date already written. */
        constructor(text: String) : this(UiText.raw(text))
    }

    /** "PG-13", boxed. */
    data class ContentRating(val rating: String) : MetaPart

    /** The critics' rating, "8.1", after a star. */
    data class Rating(val rating: String) : MetaPart

    /** "4K", "DV P8.1", "Atmos": the short quality badges (design spec §2.6). */
    data class Badges(val badges: List<String>) : MetaPart
}

/** What the main button does. */
data class PrimaryAction(
    val kind: Kind,
    /** Shows and seasons: the episode it plays, "S1 · E3". */
    val episode: String? = null,
    /** How far the item (or that episode) was played, 0 to 1. */
    val progress: Float = 0f,
    /** "1h 33m left", "26 min left"; empty unless resuming. */
    val timeLeft: String = "",
) {
    enum class Kind {
        Play,
        Resume,

        /** Nothing to play: no media files, or no episodes. */
        Unavailable,
    }
}

/**
 * The web's version, audio and subtitle selects. A list with nothing to
 * choose is empty: one version, one audio track, no subtitles.
 */
data class TrackChoices(
    /** Versions (several files), with the web's labels. */
    val versions: List<TrackChoice>,
    /** Audio tracks, when there are two or more; none is selected while the audio is automatic. */
    val audio: List<TrackChoice>,
    /** Subtitle tracks; "Off" comes before them on screen, chosen when none of them is. */
    val subtitles: List<TrackChoice>,
    /** The chosen version's id. */
    val versionId: Long?,
    /**
     * No audio track is picked: each device plays its best track of
     * [automaticLanguage] (the server picks it at play time), so the page
     * shows "Automatic (English)" first among the audio choices, ticked.
     */
    val automaticAudio: Boolean,
    /** The language automatic audio plays, "English"; `null` when the tracks don't say. */
    val automaticLanguage: String?,
    /** The picked audio track's index; `null` while the audio is automatic. */
    val audioIndex: Int?,
    /** The chosen subtitle track's index; `null`: off. */
    val subtitleIndex: Int?,
) {
    val audioLabel: String? get() = audio.firstOrNull { it.selected }?.label
    val subtitleLabel: String? get() = subtitles.firstOrNull { it.selected }?.label
    val versionLabel: String? get() = versions.firstOrNull { it.selected }?.label
}

/** One choice in a select: a version (its file id) or a track (its stream index). */
data class TrackChoice(val id: Long, val label: String, val selected: Boolean)

/** The Details list: who made it, and facts. */
data class ItemDetails(
    /** Created by, directed by, written by: up to four names each, "and N more". */
    val crew: List<CrewLine>,
    /** When it differs from the title. */
    val originalTitle: String?,
    /** "Apr 1, 2023"; not for episodes, whose date is in the metadata line. */
    val released: String?,
    val studio: String?,
    /** Episodes and shows: [studio] is the network. */
    val network: Boolean,
    /** Times played, when any. */
    val plays: Int?,
    /** Each version in the web's long form, "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB", the one that plays checked. */
    val versions: List<TrackChoice>,
) {
    val isEmpty: Boolean get() = crew.isEmpty() && originalTitle == null && released == null && studio == null && plays == null && versions.isEmpty()
}

/** "Directors: A, B, C, D and 2 more": [role], [people] (four at most) and how many more. */
data class CrewLine(val role: CreditRole, val people: List<Credit>, val more: Int) {
    /** Two or more people: the plural label. */
    val plural: Boolean get() = people.size + more > 1
}

/** The heading of the cast row. */
enum class CastTitle { Cast, GuestStars }

/** A file in Media info: its [rows] and its streams. */
data class FileInfo(
    val id: Long,
    /** Its edition, or "Version 2" when there are several; `null` for the only one. */
    val title: UiText?,
    val rows: List<InfoRow>,
    val streams: List<StreamInfo>,
    /** The file as the server describes it, for media info in full (the TV's). */
    val file: app.pley.android.model.MediaFile? = null,
)

/** A fact about a file. */
data class InfoRow(val label: InfoLabel, val value: String)

enum class InfoLabel { File, Container, Size, Duration, Bitrate, Video, Audio }

/** A stream of a file, as the web's table has it. */
data class StreamInfo(
    val kind: app.pley.android.model.StreamKind,
    val codec: String,
    /** "English", or `null`. */
    val language: String?,
    val title: String?,
    /** "4K", "3840×2160", "23.98 fps", "10-bit", "Dolby Vision P8.1", "Main 10": shown with "·" between them. */
    val details: List<UiText>,
)

/**
 * A show's seasons and one season's episodes (season tabs, the phone's
 * season picker, the episode row and list).
 */
data class SeasonsSection(
    /** The tabs: the show's seasons. Empty on an episode's page, which shows its own season only. */
    val seasons: List<SeasonTab>,
    /** The season shown. */
    val selected: Long?,
    /** Its episodes; `null` while they load. */
    val episodes: List<EpisodeLine>?,
    /** The episode to start from: the one Play plays, or the page's own episode. */
    val focus: Long?,
    /**
     * The TV's tabs show each season's poster: at least one season has a
     * poster of its own (the others show the show's). Decided for the
     * whole show, so the tabs are all posters or all words.
     */
    val posters: Boolean = false,
) {
    /** The season shown, as a tab. */
    val selectedTab: SeasonTab? get() = seasons.firstOrNull { it.id == selected }
}

/** A season among a show's: "Season 2", "Specials", with "8 episodes" and its poster (else the show's). */
data class SeasonTab(val id: Long, val title: String, val episodes: UiText?, val posterUrl: String? = null)

/** An episode in a list or row, its words worked out. */
data class EpisodeLine(
    val item: ItemSummary,
    /** "5. Glasshouse". */
    val title: String,
    /** "58 min" or "26 min left". */
    val length: String,
    /** "58 min · Mar 5, 2024", "26 min left · Mar 7, 2024". */
    val line: String,
    /** "12 March 2024", for the TV's "Aired …". */
    val aired: String?,
    /** "Season 2 · Episode 5". */
    val longLabel: String,
    /** Its runtime alone, "58 min". */
    val runtime: String,
    val summary: String?,
    val progress: Float,
    val watched: Boolean,
)
