package app.pley.android.screens.item

import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.model.CollectionRef
import app.pley.android.model.CollectionSummary
import app.pley.android.model.CreditRole
import app.pley.android.model.ExtraType
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.channelsLabel
import app.pley.android.model.format.codecLabel
import app.pley.android.model.format.dynamicRangeLabel
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.episodeLongLabel
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatDate
import app.pley.android.model.format.formatDuration
import app.pley.android.model.format.formatLongDate
import app.pley.android.model.format.formatRuntime
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.model.format.isInProgress
import app.pley.android.model.format.languageName
import app.pley.android.model.format.progressFraction
import app.pley.android.model.format.qualityBadges
import app.pley.android.model.format.resolutionLabel
import app.pley.android.model.format.seasonTitle
import app.pley.android.model.format.streamLabel
import app.pley.android.model.format.versionDescription
import app.pley.android.model.format.versionLabel
import app.pley.android.screens.R
import java.text.NumberFormat
import java.util.Locale
import kotlin.math.roundToInt

/**
 * The version and tracks picked on the page (the web's VideoItemView
 * state). A pick made for another version doesn't apply: choosing a
 * version goes back to the server's choice of tracks for it.
 */
data class StreamPick(
    /** The version picked; `null`: the first. */
    val fileId: Long? = null,
    /** The version [audio] was picked for; `null`: none was. */
    val audioFile: Long? = null,
    /** The track picked; `null` with [audioFile]: Automatic was. */
    val audio: Int? = null,
    /** The version [subtitle] was picked for; `null`: none was ([subtitle] `null` with a file: Off). */
    val subtitleFile: Long? = null,
    val subtitle: Int? = null,
)

/** What an item page is made from: the item and what was loaded beside it. */
data class PageInput(
    val detail: ItemDetail,
    val origin: String,
    val pick: StreamPick = StreamPick(),
    /** Shows and seasons: the episode Play plays; `null` while not asked. */
    val onDeck: Loadable<ItemSummary?>? = null,
    /** The show's seasons (a show's own, a season's siblings); `null` while they load. */
    val seasons: List<ItemSummary>? = null,
    val selectedSeason: Long? = null,
    /** The selected season's episodes; `null` while they load. */
    val episodes: List<ItemSummary>? = null,
    val related: List<ItemSummary> = emptyList(),
    val partOf: List<CollectionSummary> = emptyList(),
    /** The locale the page's dates and numbers are written in: the language shown's. */
    val locale: Locale = AppLanguage.locale,
    /** The words of its runtimes, episode numbers and tracks: the language shown's. */
    val words: FormatWords = AppLanguage.formatWords(),
)

/**
 * The file the page plays, and the tracks it starts with. [pickedAudio] is
 * the audio picked on the page for that file, the only one the player and
 * the saved choice are told of: otherwise the server picks the track each
 * device plays best, and [audioIndex] is the details' choice, made for no
 * device in particular. [automaticAudio]: no track is picked, on the page
 * or before, so the page names only [audioIndex]'s language.
 */
internal data class Playing(
    val file: MediaFile?,
    val audioIndex: Int?,
    val subtitleIndex: Int?,
    val defaultAudio: MediaStream?,
    val pickedAudio: Int? = null,
    val automaticAudio: Boolean = false,
)

/** The version and tracks [pick] comes to on [detail], as the web works them out. */
internal fun playing(detail: ItemDetail, pick: StreamPick): Playing {
    val file = detail.files.firstOrNull { it.id == pick.fileId } ?: detail.files.firstOrNull()
    val audio = file?.streams?.filter { it.kind == StreamKind.Audio }.orEmpty()
    val defaultAudio = audio.firstOrNull { it.default } ?: audio.firstOrNull()
    val pickedHere = file != null && pick.audioFile == file.id
    val pickedAudio = if (pickedHere) pick.audio else null
    val automatic = if (pickedHere) pick.audio == null else file?.audioPicked != true
    val audioIndex = pickedAudio ?: file?.selectedAudio
    val subtitleIndex = if (file != null && pick.subtitleFile == file.id) pick.subtitle else file?.selectedSubtitle
    return Playing(file, audioIndex, subtitleIndex, defaultAudio, pickedAudio = pickedAudio, automaticAudio = automatic)
}

/** The page of [input]'s item. */
fun itemPage(input: PageInput): ItemPage {
    val d = input.detail
    val locale = input.locale
    val words = input.words
    val playing = playing(d, input.pick)
    val onDeck = input.onDeck?.valueOrNull
    val kind = d.kind
    val video = kind == ItemKind.Movie || kind == ItemKind.Episode || kind == ItemKind.Extra
    val container = kind == ItemKind.Show || kind == ItemKind.Season
    val resumable = video && !d.userState.watched && d.userState.positionMs > 0
    val primary = when {
        video && d.files.isEmpty() -> PrimaryAction(PrimaryAction.Kind.Unavailable)
        video && resumable -> PrimaryAction(
            PrimaryAction.Kind.Resume,
            progress = progressFraction(d.userState.positionMs, d.durationMs),
            timeLeft = formatTimeLeft(d.userState.positionMs, d.durationMs, words),
        )
        video -> PrimaryAction(PrimaryAction.Kind.Play)
        else -> containerPrimary(d, onDeck, input.episodes.takeIf { kind == ItemKind.Season && input.selectedSeason == d.id }, words)
    }
    val watched = if (container) {
        val leaves = d.leafCount ?: 0
        leaves > 0 && (d.viewedLeafCount ?: 0) >= leaves
    } else {
        d.userState.watched
    }
    return ItemPage(
        id = d.id,
        kind = kind,
        origin = input.origin,
        item = d.toSummary(),
        title = if (kind == ItemKind.Season) seasonTitle(d.title, d.index, words) else d.title,
        context = context(d, words),
        meta = meta(d, playing.file, input.seasons, locale, words),
        genres = d.genres,
        partOf = input.partOf.map { CollectionRef(it.id, it.title) },
        primary = primary,
        fromStart = primary.kind == PrimaryAction.Kind.Resume,
        watched = watched,
        myList = if (kind.listable) d.inWatchlist else null,
        userRating = d.userState.rating?.roundToInt()?.coerceIn(0, 10)?.takeIf { it > 0 },
        rateable = kind == ItemKind.Movie || kind == ItemKind.Show || kind == ItemKind.Episode,
        tagline = d.tagline?.takeIf { it.isNotBlank() },
        summary = d.summary?.takeIf { it.isNotBlank() },
        tracks = trackChoices(d, playing, words),
        trailer = d.extras.firstOrNull { it.extraType == ExtraType.Trailer },
        details = details(d, playing.file, locale, words),
        mediaInfo = mediaInfo(d.files, locale, words),
        extras = d.extras.distinctBy { it.id },
        castTitle = if (kind == ItemKind.Episode) CastTitle.GuestStars else CastTitle.Cast,
        cast = d.cast.distinctBy { it.id },
        related = if (kind == ItemKind.Movie || kind == ItemKind.Show) input.related.distinctBy { it.id }.filter { it.id != d.id } else emptyList(),
        seasons = seasons(input, onDeck, locale, words),
    )
}

/** A show's or season's button: its next episode, "Resume S2 · E5" or "Play S1 · E1". */
private fun containerPrimary(d: ItemDetail, onDeck: ItemSummary?, ownEpisodes: List<ItemSummary>?, words: FormatWords): PrimaryAction {
    if (onDeck == null) {
        val nothing = when (d.kind) {
            ItemKind.Season -> ownEpisodes?.isEmpty() == true || d.leafCount == 0
            else -> d.leafCount == null || d.leafCount == 0
        }
        return PrimaryAction(if (nothing) PrimaryAction.Kind.Unavailable else PrimaryAction.Kind.Play)
    }
    val resuming = onDeck.userState.positionMs > 0 && !onDeck.userState.watched
    return PrimaryAction(
        kind = if (resuming) PrimaryAction.Kind.Resume else PrimaryAction.Kind.Play,
        episode = episodeLabel(onDeck, words).ifEmpty { null },
        progress = if (resuming) progressFraction(onDeck) else 0f,
        timeLeft = if (resuming) formatTimeLeft(onDeck.userState.positionMs, onDeck.durationMs, words) else "",
    )
}

private fun context(d: ItemDetail, words: FormatWords): ItemContext? = when (d.kind) {
    ItemKind.Episode -> ItemContext(
        showId = d.grandparentId,
        showTitle = d.grandparentTitle?.takeIf { it.isNotEmpty() },
        seasonId = d.parentId,
        seasonTitle = d.parentIndex?.let { seasonTitle(null, it, words) } ?: d.parentTitle?.takeIf { it.isNotEmpty() },
        episodeLabel = episodeLabel(d.parentIndex, d.index, words).ifEmpty { null },
        episodeLongLabel = episodeLongLabel(d.parentIndex, d.index, words).ifEmpty { null },
    )
    ItemKind.Season -> ItemContext(
        showId = d.parentId,
        showTitle = d.parentTitle?.takeIf { it.isNotEmpty() },
        seasonId = null,
        seasonTitle = null,
        episodeLabel = null,
        episodeLongLabel = null,
    )
    else -> null
}

/** The metadata line (design spec §2.2, §4.3, §4.4). */
private fun meta(d: ItemDetail, file: MediaFile?, seasons: List<ItemSummary>?, locale: Locale, words: FormatWords): List<MetaPart> = buildList {
    fun text(value: String?) {
        if (!value.isNullOrBlank()) add(MetaPart.Text(value))
    }
    when (d.kind) {
        ItemKind.Show -> {
            val count = d.childCount ?: seasons?.count { (it.index ?: 1) > 0 }
            if (count != null && count > 0) add(MetaPart.Text(UiText.plural(R.plurals.item_season_count, count)))
            text(yearRange(d.year, seasons))
        }
        ItemKind.Season -> {
            text(d.year?.toString())
            d.leafCount?.takeIf { it > 0 }?.let { add(MetaPart.Text(UiText.plural(R.plurals.item_episode_count, it))) }
        }
        ItemKind.Episode -> {
            text(formatDate(d.releaseDate, locale))
            text(file?.edition)
            text(formatRuntime(d.durationMs, words))
        }
        else -> {
            text(d.year?.toString())
            text(file?.edition)
            text(formatRuntime(d.durationMs, words))
        }
    }
    d.contentRating?.takeIf { it.isNotBlank() }?.let { add(MetaPart.ContentRating(it)) }
    d.rating?.takeIf { it > 0 }?.let { add(MetaPart.Rating(String.format(locale, "%.1f", it))) }
    val badges = qualityBadges(file)
    if (badges.isNotEmpty()) add(MetaPart.Badges(badges))
}

/** "2021-2025" from a show's seasons, or its own year. */
private fun yearRange(year: Int?, seasons: List<ItemSummary>?): String? {
    val years = seasons.orEmpty().mapNotNull { it.year } + listOfNotNull(year)
    val first = years.minOrNull() ?: return null
    val last = years.max()
    return if (last > first) "$first-$last" else first.toString()
}

/** The selects (VideoItemView.tsx): versions when there are several, audio when two or more, subtitles when any. */
private fun trackChoices(d: ItemDetail, playing: Playing, words: FormatWords): TrackChoices? {
    val file = playing.file ?: return null
    val audio = file.streams.filter { it.kind == StreamKind.Audio }
    val subtitles = file.streams.filter { it.kind == StreamKind.Subtitle }
    if (d.files.size < 2 && audio.size < 2 && subtitles.isEmpty()) return null
    val audioStart = playing.audioIndex ?: playing.defaultAudio?.index
    // Without a pick no track is named: each device plays its best of this one's language.
    val audioShown = audioStart.takeUnless { playing.automaticAudio }
    return TrackChoices(
        versions = if (d.files.size > 1) {
            d.files.mapIndexed { i, f -> TrackChoice(f.id, versionLabel(f, i, words), f.id == file.id) }
        } else {
            emptyList()
        },
        audio = if (audio.size > 1) audio.map { TrackChoice(it.index.toLong(), streamLabel(it, words), it.index == audioShown) } else emptyList(),
        subtitles = subtitles.map { TrackChoice(it.index.toLong(), streamLabel(it, words), it.index == playing.subtitleIndex) },
        versionId = file.id,
        automaticAudio = playing.automaticAudio,
        automaticLanguage = languageName(audio.firstOrNull { it.index == audioStart }?.language, words),
        audioIndex = audioShown,
        subtitleIndex = playing.subtitleIndex?.takeIf { index -> subtitles.any { it.index == index } },
    )
}

private val CREW_ROLES = listOf(CreditRole.Creator, CreditRole.Director, CreditRole.Writer)

/** At most this many names per crew line, then "and N more" (Credits.tsx). */
const val CREW_NAMES = 4

private fun details(d: ItemDetail, file: MediaFile?, locale: Locale, words: FormatWords): ItemDetails = ItemDetails(
    crew = CREW_ROLES.mapNotNull { role ->
        val people = d.crew.filter { it.role == role }.distinctBy { it.id }
        if (people.isEmpty()) null else CrewLine(role, people.take(CREW_NAMES), (people.size - CREW_NAMES).coerceAtLeast(0))
    },
    originalTitle = d.originalTitle?.takeIf { it.isNotBlank() && it != d.title },
    released = d.releaseDate?.takeIf { d.kind != ItemKind.Episode && it.isNotBlank() }?.let { formatDate(it, locale) },
    studio = d.studio?.takeIf { it.isNotBlank() },
    network = d.kind == ItemKind.Episode || d.kind == ItemKind.Show,
    plays = d.userState.playCount.takeIf { it > 0 },
    versions = d.files.map { TrackChoice(it.id, versionDescription(it, words), it.id == file?.id) },
)

/** Media info (MediaInfo.tsx). An app's token gets file names only; whatever comes, only the name shows. */
private fun mediaInfo(files: List<MediaFile>, locale: Locale, words: FormatWords): List<FileInfo> = files.mapIndexed { i, f ->
    val video = f.streams.firstOrNull { it.kind == StreamKind.Video }
    FileInfo(
        id = f.id,
        title = f.edition?.takeIf { it.isNotBlank() }?.let(UiText::raw) ?: if (files.size > 1) UiText.res(R.string.item_media_version, i + 1) else null,
        rows = buildList {
            add(InfoRow(InfoLabel.File, fileName(f.path)))
            add(InfoRow(InfoLabel.Container, f.container?.uppercase() ?: "—"))
            if (f.size > 0) add(InfoRow(InfoLabel.Size, formatBytes(f.size, words)))
            f.durationMs?.takeIf { it > 0 }?.let { add(InfoRow(InfoLabel.Duration, formatDuration(it, words))) }
            f.bitrate?.takeIf { it > 0 }?.let { add(InfoRow(InfoLabel.Bitrate, formatBitrate(it, words))) }
            if (!f.videoCodec.isNullOrEmpty()) {
                val value = listOfNotNull(codecLabel(f.videoCodec), resolutionLabel(f.width, f.height), dynamicRangeLabel(video))
                    .filter { it.isNotEmpty() }.joinToString(" · ")
                add(InfoRow(InfoLabel.Video, value))
            }
            if (!f.audioCodec.isNullOrEmpty()) add(InfoRow(InfoLabel.Audio, codecLabel(f.audioCodec)))
        },
        streams = f.streams
            .filter { it.kind != StreamKind.Unknown }
            .sortedWith(compareBy({ it.kind.ordinal }, { it.index }))
            .map { s -> StreamInfo(s.kind, codecLabel(s.codec), languageName(s.language, words), s.title?.takeIf { it.isNotBlank() }, streamDetails(s, locale, words)) },
        file = f,
    )
}

/** The last part of a path, whichever separator it uses. */
fun fileName(path: String): String = path.substringAfterLast('/').substringAfterLast('\\')

private fun streamDetails(s: MediaStream, locale: Locale, words: FormatWords): List<UiText> {
    val parts = mutableListOf<UiText>()
    fun add(text: String) {
        parts += UiText.raw(text)
    }
    when (s.kind) {
        StreamKind.Video -> {
            resolutionLabel(s.width, s.height)?.let(::add)
            val w = s.width ?: 0
            val h = s.height ?: 0
            if (w > 0 && h > 0) add("$w×$h")
            s.frameRate?.takeIf { it > 0 }?.let { parts += UiText.res(R.string.item_media_fps, frameRate(it, locale)) }
            s.bitDepth?.takeIf { it > 0 }?.let { parts += UiText.res(R.string.item_media_bit_depth, it) }
            dynamicRangeLabel(s)?.let(::add)
            s.profile?.takeIf { it.isNotBlank() }?.let(::add)
        }
        StreamKind.Audio -> {
            channelsLabel(s.channels, s.channelLayout, words).takeIf { it.isNotEmpty() }?.let(::add)
            s.sampleRate?.takeIf { it > 0 }?.let { parts += UiText.res(R.string.item_media_khz, it / 1000.0) }
        }
        else -> {
            parts += UiText.res(if (s.textBased == false) R.string.item_media_image_based else R.string.item_media_text)
            if (s.external) parts += UiText.res(R.string.item_media_external)
        }
    }
    s.bitrate?.takeIf { it > 0 }?.let { add(formatBitrate(it, words)) }
    if (s.forced) parts += UiText.res(R.string.item_media_forced)
    if (s.default) parts += UiText.res(R.string.item_media_default)
    return parts
}

/** 23.976 → "23.98", 24.0 → "24", in [locale]'s digits ("23,98" in French). */
private fun frameRate(fps: Double, locale: Locale): String =
    NumberFormat.getNumberInstance(locale).apply { maximumFractionDigits = 2 }.format(Math.round(fps * 100) / 100.0)

private fun seasons(input: PageInput, onDeck: ItemSummary?, locale: Locale, words: FormatWords): SeasonsSection? {
    val d = input.detail
    val tabs = when (d.kind) {
        ItemKind.Show, ItemKind.Season -> input.seasons.orEmpty().map { season ->
            SeasonTab(season.id, seasonTitle(season.title, season.index, words), season.leafCount?.let { UiText.plural(R.plurals.item_episode_count, it) })
        }
        ItemKind.Episode -> emptyList()
        else -> return null
    }
    val focus = when (d.kind) {
        ItemKind.Episode -> d.id
        else -> onDeck?.takeIf { it.parentId == input.selectedSeason }?.id
    }
    return SeasonsSection(
        seasons = tabs,
        selected = input.selectedSeason,
        episodes = input.episodes?.distinctBy { it.id }?.map { episodeLine(it, locale, words) },
        focus = focus,
    )
}

/** An episode's words for lists and rows, its dates in [locale], its runtime and numbers in [words]. */
fun episodeLine(e: ItemSummary, locale: Locale = AppLanguage.locale, words: FormatWords = AppLanguage.formatWords()): EpisodeLine {
    val inProgress = isInProgress(e)
    val runtime = formatRuntime(e.durationMs, words)
    val length = if (inProgress) formatTimeLeft(e.userState.positionMs, e.durationMs, words) else runtime
    return EpisodeLine(
        item = e,
        title = if (e.index != null) "${e.index}. ${e.title}" else e.title,
        length = length,
        line = listOf(length, formatDate(e.releaseDate, locale)).filter { it.isNotEmpty() }.joinToString(" · "),
        aired = formatLongDate(e.releaseDate, locale).ifEmpty { null },
        longLabel = episodeLongLabel(e.parentIndex, e.index, words),
        runtime = runtime,
        summary = e.summary?.takeIf { it.isNotBlank() },
        progress = if (inProgress) progressFraction(e) else 0f,
        watched = e.userState.watched,
    )
}

/** The season a show page opens on: the next episode's, else the first that isn't Specials, else the first. */
fun defaultSeason(seasons: List<ItemSummary>, onDeck: ItemSummary?): Long? =
    onDeck?.parentId?.takeIf { id -> seasons.any { it.id == id } }
        ?: seasons.firstOrNull { (it.index ?: 1) > 0 }?.id
        ?: seasons.firstOrNull()?.id

private val EXTRA_LABELS = mapOf(
    ExtraType.Trailer to R.string.item_extra_trailer,
    ExtraType.BehindTheScenes to R.string.item_extra_behind_the_scenes,
    ExtraType.DeletedScene to R.string.item_extra_deleted_scene,
    ExtraType.Featurette to R.string.item_extra_featurette,
    ExtraType.Interview to R.string.item_extra_interview,
    ExtraType.Scene to R.string.item_extra_scene,
    ExtraType.Short to R.string.item_extra_short,
)

/** An extra's caption line, as the web's Extras row: "Trailer · 2m", its length in [words]. */
fun extraSubtitle(item: ItemSummary, words: FormatWords): UiText {
    val kind = UiText.res(EXTRA_LABELS[item.extraType] ?: R.string.item_extra_other)
    val length = formatDuration(item.durationMs, words)
    return if (length.isEmpty()) kind else UiText.res(R.string.item_extra_caption, kind, length)
}
