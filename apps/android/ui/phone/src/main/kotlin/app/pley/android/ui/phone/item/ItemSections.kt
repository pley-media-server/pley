package app.pley.android.ui.phone.item

import android.content.res.Resources
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ClipOp
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import app.pley.android.common.formatWords
import app.pley.android.common.resolve
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTile
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonState
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.StreamKind
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.EpisodeLine
import app.pley.android.screens.item.FileInfo
import app.pley.android.screens.item.InfoLabel
import app.pley.android.screens.item.ItemPage
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.screens.item.SeasonsSection
import app.pley.android.screens.item.TrackChoice
import app.pley.android.screens.item.extraSubtitle
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.downloads.DownloadIconButton
import app.pley.android.ui.phone.downloads.DownloadMenu
import app.pley.android.ui.phone.kit.HubRowModel
import app.pley.android.ui.phone.kit.PleyIconButton

/** What the page's parts can do: the page's model and the navigation. */
@Stable
internal class PageActions(private val model: ItemViewModel, private val navigator: Navigator) {
    fun play(fromStart: Boolean) = model.play(fromStart)

    fun play(item: ItemSummary) = model.play(item)

    fun toggleWatched() = model.toggleWatched()

    fun toggleMyList() = model.toggleMyList()

    fun setWatched(id: Long, watched: Boolean) = model.setWatched(id, watched)

    fun rate(rating: Int?) = model.rate(rating)

    fun chooseVersion(fileId: Long) = model.chooseVersion(fileId)

    /** [index] `null`: Automatic. */
    fun chooseAudio(index: Int?) = model.chooseAudio(index)

    fun chooseSubtitle(index: Int?) = model.chooseSubtitle(index)

    fun selectSeason(id: Long) = model.selectSeason(id)

    fun openItem(id: Long) = navigator.open(PleyKey.Item(id))

    fun openPerson(id: Long) = navigator.open(PleyKey.Person(id))

    fun openCollection(id: Long) = navigator.open(PleyKey.Collection(id))

    fun addToPlaylist(id: Long) = navigator.open(PleyKey.AddToPlaylist(listOf(id)))

    fun showActions(id: Long) = navigator.open(PleyKey.ItemActions(id))
}

/** The rows' headings, in the page's language. */
@Immutable
internal data class RowTitles(val extras: String, val cast: String, val related: String, val season: String)

/** A row and what tapping (and holding) one of its tiles does. */
@Immutable
internal class TileRow(
    val model: HubRowModel,
    val open: (Long, PageActions) -> Unit,
    val longPress: ((Long, PageActions) -> Unit)? = null,
)

/** The page's rows: extras, an episode's season, the cast, More Like This. Built once per page, in the language of [resources]. */
internal fun phoneRows(page: ItemPage, titles: RowTitles, resources: Resources): List<TileRow> = buildList {
    val origin = page.origin
    val words = formatWords(resources)
    if (page.extras.isNotEmpty()) {
        val byId = page.extras.associateBy { it.id }
        add(
            TileRow(
                HubRowModel(
                    "extras",
                    titles.extras,
                    TileKind.Landscape,
                    page.extras.map { ArtworkTileModel.of(it, TileKind.Landscape, origin, words).copy(subtitle = extraSubtitle(it, words).resolve(resources)) },
                ),
                open = { id, actions -> byId[id]?.let(actions::play) },
            ),
        )
    }
    val episodes = page.seasons?.episodes
    if (page.kind == ItemKind.Episode && !episodes.isNullOrEmpty()) {
        add(
            TileRow(
                HubRowModel("season", titles.season, TileKind.Landscape, episodes.map { ArtworkTileModel.of(it.item, TileKind.Episode, origin, words) }),
                open = { id, actions -> actions.openItem(id) },
                longPress = { id, actions -> actions.showActions(id) },
            ),
        )
    }
    if (page.cast.isNotEmpty()) {
        add(
            TileRow(
                HubRowModel(
                    "cast",
                    titles.cast,
                    TileKind.Person,
                    page.cast.map { ArtworkTileModel(id = it.id, title = it.name, subtitle = it.character.orEmpty(), image = ArtworkImage.of(it.photoUrl, origin)) },
                ),
                open = { id, actions -> actions.openPerson(id) },
            ),
        )
    }
    if (page.related.isNotEmpty()) {
        add(
            TileRow(
                HubRowModel("related", titles.related, TileKind.Poster, page.related.map { ArtworkTileModel.of(it, TileKind.Poster, origin, words) }),
                open = { id, actions -> actions.openItem(id) },
                longPress = { id, actions -> actions.showActions(id) },
            ),
        )
    }
}

/**
 * Version, Audio and Subtitles (VideoItemView.tsx's selects; design spec
 * §4.3): outlined fields, Audio and Subtitles side by side and as tall as
 * each other. Choosing a track is kept for the item. Audio starts with
 * Automatic and its language, ticked while no track is picked: each device
 * then plays its best track of that language.
 */
@Composable
internal fun TrackSelects(page: ItemPage, actions: PageActions, modifier: Modifier = Modifier) {
    val tracks = page.tracks ?: return
    Column(modifier.fillMaxWidth().padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (tracks.versions.isNotEmpty()) {
            Select(
                label = stringResource(R.string.item_version),
                value = tracks.versionLabel.orEmpty(),
                options = tracks.versions.map { it.id to it.label },
                selected = tracks.versionId,
                onSelect = { id -> id?.let(actions::chooseVersion) },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        if (tracks.audio.isNotEmpty() || tracks.subtitles.isNotEmpty()) {
            Row(Modifier.height(IntrinsicSize.Min), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                if (tracks.audio.isNotEmpty()) {
                    val automatic = automaticAudioLabel(tracks.automaticLanguage)
                    Select(
                        label = stringResource(R.string.item_audio),
                        value = if (tracks.automaticAudio) automatic else tracks.audioLabel.orEmpty(),
                        options = listOf<Pair<Long?, String>>(null to automatic) + tracks.audio.map { it.id to it.label },
                        selected = tracks.audioIndex?.toLong(),
                        onSelect = { id -> actions.chooseAudio(id?.toInt()) },
                        modifier = Modifier.weight(1f).fillMaxHeight(),
                    )
                }
                if (tracks.subtitles.isNotEmpty()) {
                    val off = stringResource(R.string.item_subtitles_off)
                    Select(
                        label = stringResource(R.string.item_subtitles),
                        value = tracks.subtitleLabel ?: off,
                        options = listOf<Pair<Long?, String>>(null to off) + tracks.subtitles.map { it.id to it.label },
                        selected = tracks.subtitleIndex?.toLong(),
                        onSelect = { id -> actions.chooseSubtitle(id?.toInt()) },
                        modifier = Modifier.weight(1f).fillMaxHeight(),
                    )
                }
            }
        }
    }
}

/** "Automatic (English)", or "Automatic" when the tracks don't say their language. */
@Composable
private fun automaticAudioLabel(language: String?): String =
    if (language != null) stringResource(R.string.item_audio_automatic_language, language) else stringResource(R.string.item_audio_automatic)

/**
 * An outlined select: the [label] notched into its edge, the [value] on up
 * to two lines, a menu of [options] with the [selected] one ticked.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Select(
    label: String,
    value: String,
    options: List<Pair<Long?, String>>,
    selected: Long?,
    onSelect: (Long?) -> Unit,
    modifier: Modifier = Modifier,
) {
    var open by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = open, onExpandedChange = { open = it }, modifier = modifier) {
        val focus = remember { MutableInteractionSource() }
        val focused by focus.collectIsFocusedAsState()
        SelectField(
            label = label,
            value = value,
            active = open || focused,
            modifier = Modifier
                .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable)
                .focusable(interactionSource = focus)
                .semantics(mergeDescendants = true) { }
                .fillMaxWidth()
                .fillMaxHeight(),
        )
        ExposedDropdownMenu(
            expanded = open,
            onDismissRequest = { open = false },
            containerColor = PleyColors.Surface2,
        ) {
            options.forEach { (id, text) ->
                DropdownMenuItem(
                    text = { Text(text, style = MaterialTheme.typography.bodyLarge) },
                    onClick = {
                        open = false
                        onSelect(id)
                    },
                    leadingIcon = {
                        Box(Modifier.size(20.dp)) {
                            if (id == selected) Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
                        }
                    },
                    colors = MenuDefaults.itemColors(textColor = PleyColors.Fg),
                )
            }
        }
    }
}

/**
 * The select's box: a 1 dp outline (2 dp in the accent while [active])
 * with the [label] sitting in a gap in its top edge, the [value] wrapping
 * to a second line before it ends in an ellipsis, and a chevron.
 */
@Composable
private fun SelectField(label: String, value: String, active: Boolean, modifier: Modifier = Modifier) {
    val labelStyle = MaterialTheme.typography.bodySmall
    // The label's middle is on the top edge.
    val rise = with(LocalDensity.current) { labelStyle.lineHeight.toDp() } / 2
    var notch by remember { mutableIntStateOf(0) }
    val edge = if (active) PleyColors.AccentBright else PleyColors.Control
    Box(modifier) {
        Row(
            Modifier
                .fillMaxSize()
                .padding(top = rise)
                .heightIn(min = FIELD_HEIGHT)
                .drawWithCache {
                    val stroke = Stroke((if (active) 2 else 1).dp.toPx())
                    val inset = stroke.width / 2
                    val outline = PleyShape.Field.createOutline(Size(size.width - stroke.width, size.height - stroke.width), layoutDirection, this)
                    val gapStart = NOTCH_START.toPx()
                    onDrawBehind {
                        val left = if (layoutDirection == LayoutDirection.Ltr) gapStart else size.width - gapStart - notch
                        clipRect(left, -stroke.width, left + notch, stroke.width * 2, ClipOp.Difference) {
                            translate(inset, inset) { drawOutline(outline, edge, style = stroke) }
                        }
                    }
                }
                .padding(start = 16.dp, end = 12.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                value,
                style = MaterialTheme.typography.bodyLarge,
                color = PleyColors.Fg,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            Icon(PleyIcons.ChevronDown, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
        }
        Text(
            label,
            style = labelStyle,
            color = if (active) PleyColors.AccentBright else PleyColors.Muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .padding(start = NOTCH_START, end = 16.dp)
                .onSizeChanged { notch = it.width }
                .padding(horizontal = 4.dp),
        )
    }
}

/** A select's height under its label's rise, as Material's outlined field. */
private val FIELD_HEIGHT = 56.dp

/** Where the gap for a select's label starts along its top edge. */
private val NOTCH_START = 12.dp

/**
 * The season picker over the episode list (design spec §3.4, §4.4): a
 * tonal pill opening the show's seasons, with "8 episodes" at the right.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SeasonPicker(seasons: SeasonsSection, actions: PageActions, modifier: Modifier = Modifier) {
    val tab = seasons.selectedTab
    val count = tab?.episodes
    Row(modifier.fillMaxWidth().padding(top = PleyPhone.SectionGap, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        if (seasons.seasons.size > 1 && tab != null) {
            var open by remember { mutableStateOf(false) }
            val choose = stringResource(R.string.item_choose_season)
            ExposedDropdownMenuBox(expanded = open, onExpandedChange = { open = it }) {
                Row(
                    Modifier
                        .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable)
                        .heightIn(min = 40.dp)
                        .background(PleyColors.Surface3, PleyShape.Control)
                        .padding(start = 20.dp, end = 14.dp)
                        .semantics { heading() },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(tab.title, style = MaterialTheme.typography.labelLarge, color = PleyColors.Fg)
                    Spacer(Modifier.width(8.dp))
                    Icon(PleyIcons.ChevronDown, contentDescription = choose, size = 20.dp, tint = PleyColors.Fg)
                }
                ExposedDropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = PleyColors.Surface2) {
                    seasons.seasons.forEach { season ->
                        DropdownMenuItem(
                            text = { Text(season.title, style = MaterialTheme.typography.bodyLarge) },
                            trailingIcon = season.episodes?.let { { Text(it.asString(), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted) } },
                            leadingIcon = {
                                Box(Modifier.size(20.dp)) {
                                    if (season.id == seasons.selected) Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
                                }
                            },
                            onClick = {
                                open = false
                                actions.selectSeason(season.id)
                            },
                            colors = MenuDefaults.itemColors(textColor = PleyColors.Fg),
                        )
                    }
                }
            }
        } else {
            Text(
                tab?.title ?: stringResource(R.string.item_episodes),
                style = MaterialTheme.typography.titleLarge,
                color = PleyColors.Fg,
                modifier = Modifier.semantics { heading() },
            )
        }
        Spacer(Modifier.weight(1f))
        if (count != null) Text(count.asString(), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        SeasonDownload(seasons.episodes.orEmpty().map { it.item.id })
    }
}

/** Downloads a season's episodes not downloaded yet; gone once they all are. */
@Composable
private fun SeasonDownload(episodeIds: List<Long>) {
    val downloads = LocalItemDownloads.current ?: return
    if (episodeIds.isEmpty() || downloads.hasAll(episodeIds)) return
    PleyIconButton(
        icon = PleyIcons.Download,
        label = stringResource(R.string.downloads_download_season),
        onClick = { downloads.download(episodeIds) },
        modifier = Modifier.padding(start = 4.dp),
    )
}

/** An episode row's download button, beside the watched tick. */
@Composable
private fun EpisodeDownload(itemId: Long, title: String) {
    val downloads = LocalItemDownloads.current ?: return
    val entry = downloads.of(itemId)
    var menu by remember { mutableStateOf(false) }
    Box {
        DownloadIconButton(
            entry = entry,
            title = title,
            onDownload = { downloads.download(listOf(itemId)) },
            onMore = { menu = true },
            modifier = Modifier.padding(start = 4.dp).size(36.dp),
        )
        DownloadMenu(
            entry,
            expanded = menu,
            onDismiss = { menu = false },
            onPlay = downloads.play,
            onRetry = { downloads.retry(it.itemId) },
            onRemove = { downloads.remove(it.itemId) },
        )
    }
}

/**
 * An episode in the list (design spec §4.4, mockup phone-show): a 128 by
 * 72 still with its progress, tick and a play circle (tapping it plays),
 * then "5. Glasshouse", its length and date, and two lines of summary
 * (tapping them opens the episode). The arrow downloads it (its ring says
 * how far; a downloaded one's menu plays or deletes it) and the check
 * toggles watched; holding either part opens the episode's actions.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun EpisodeRow(episode: EpisodeLine, origin: String, actions: PageActions, modifier: Modifier = Modifier) {
    val item = episode.item
    val words = formatWords()
    val tile = remember(item, origin, words) { ArtworkTileModel.of(item, TileKind.Episode, origin, words) }
    val playLabel = stringResource(R.string.item_play_named, episode.title)
    Row(modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Box(
            Modifier
                .width(EpisodeStillWidth)
                .aspectRatio(16f / 9f)
                .clip(PleyShape.TilePhone)
                .combinedClickable(
                    role = Role.Button,
                    onClickLabel = playLabel,
                    onLongClick = { actions.showActions(item.id) },
                    onClick = { actions.play(item) },
                ),
            contentAlignment = Alignment.Center,
        ) {
            ArtworkTile(tile, TileKind.Episode, TileStyle.Phone, Modifier.matchParentSize())
            Box(Modifier.size(32.dp).background(PleyColors.OnArt, PleyShape.Circle), contentAlignment = Alignment.Center) {
                Icon(PleyIcons.Play, contentDescription = null, size = 16.dp, tint = PleyColors.Fg)
            }
        }
        Spacer(Modifier.width(12.dp))
        Column(
            Modifier
                .weight(1f)
                .combinedClickable(role = Role.Button, onLongClick = { actions.showActions(item.id) }, onClick = { actions.openItem(item.id) }),
        ) {
            Row(verticalAlignment = Alignment.Top) {
                Text(
                    episode.title,
                    style = MaterialTheme.typography.titleMedium,
                    color = PleyColors.Fg,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).padding(top = 2.dp),
                )
                EpisodeDownload(item.id, episode.title)
                PleyIconButton(
                    icon = if (episode.watched) PleyIcons.CircleCheck else PleyIcons.Check,
                    label = stringResource(if (episode.watched) R.string.item_mark_unwatched else R.string.item_mark_watched),
                    onClick = { actions.setWatched(item.id, !episode.watched) },
                    tint = if (episode.watched) PleyColors.Accent else PleyColors.Control,
                    modifier = Modifier.padding(start = 4.dp).size(36.dp),
                )
            }
            if (episode.line.isNotEmpty()) Text(episode.line, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            episode.summary?.let {
                Spacer(Modifier.height(4.dp))
                Text(it, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg.copy(alpha = 0.85f), maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** An episode row while the list loads. */
@Composable
internal fun EpisodeRowSkeleton(skeleton: SkeletonState, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Box(Modifier.width(EpisodeStillWidth).aspectRatio(16f / 9f).skeleton(skeleton, PleyShape.TilePhone))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.fillMaxWidth(0.6f).height(16.dp).skeleton(skeleton, PleyShape.Badge))
            Box(Modifier.fillMaxWidth(0.35f).height(12.dp).skeleton(skeleton, PleyShape.Badge))
            Box(Modifier.fillMaxWidth(0.9f).height(12.dp).skeleton(skeleton, PleyShape.Badge))
        }
    }
}

/**
 * Details (VideoItemView.tsx's list): who created, directed and wrote it
 * (names open their pages), the original title, the release, the studio
 * or network, plays, and each version in its long form.
 */
@Composable
internal fun DetailsSection(page: ItemPage, actions: PageActions, modifier: Modifier = Modifier) {
    val details = page.details
    Column(modifier.fillMaxWidth().padding(top = PleyPhone.SectionGap)) {
        SectionTitle(stringResource(R.string.item_details))
        Spacer(Modifier.height(PleyPhone.HeaderToContent))
        details.crew.forEach { line ->
            val label = when (line.role) {
                CreditRole.Creator -> stringResource(R.string.item_created_by)
                CreditRole.Director -> stringResource(if (line.plural) R.string.item_directors else R.string.item_director)
                else -> stringResource(if (line.plural) R.string.item_writers else R.string.item_writer)
            }
            val more = if (line.more > 0) pluralStringResource(R.plurals.item_and_more, line.more, line.more) else null
            val text = remember(line, more) {
                buildAnnotatedString {
                    line.people.forEachIndexed { i, person ->
                        if (i > 0) append(", ")
                        withLink(
                            LinkAnnotation.Clickable(
                                tag = "person-${person.id}",
                                styles = TextLinkStyles(SpanStyle(color = PleyColors.Fg)),
                            ) { actions.openPerson(person.id) },
                        ) { append(person.name) }
                    }
                    if (more != null) withStyle(SpanStyle(color = PleyColors.Muted)) { append(" $more") }
                }
            }
            DetailRow(label, text)
        }
        details.originalTitle?.let { DetailRow(stringResource(R.string.item_original_title), AnnotatedString(it)) }
        details.released?.let { DetailRow(stringResource(R.string.item_released), AnnotatedString(it)) }
        details.studio?.let { DetailRow(stringResource(if (details.network) R.string.item_network else R.string.item_studio), AnnotatedString(it)) }
        details.plays?.let { DetailRow(stringResource(R.string.item_plays), AnnotatedString(it.toString())) }
        if (details.versions.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            Text(stringResource(R.string.item_versions), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
            details.versions.forEach { VersionLine(it) }
        }
    }
}

@Composable
private fun DetailRow(label: String, value: AnnotatedString) {
    Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
        Text(label, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle, modifier = Modifier.width(DetailLabelWidth))
        Text(value, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg.copy(alpha = 0.85f), modifier = Modifier.weight(1f))
    }
}

@Composable
private fun VersionLine(version: TrackChoice) {
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.Top) {
        Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
            if (version.selected) Icon(PleyIcons.Check, contentDescription = null, size = 18.dp, tint = PleyColors.Accent)
        }
        Spacer(Modifier.width(8.dp))
        Text(version.label, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg.copy(alpha = 0.85f))
    }
}

/** "Media info", collapsed (MediaInfo.tsx): each file's facts and streams. File names only, never where the server keeps them. */
@Composable
internal fun MediaInfoSection(files: List<FileInfo>, modifier: Modifier = Modifier) {
    var open by rememberSaveable { mutableStateOf(false) }
    val resources = LocalResources.current
    Column(modifier.fillMaxWidth().padding(top = PleyPhone.SectionGap)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(role = Role.Button) { open = !open },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(if (open) PleyIcons.ChevronUp else PleyIcons.ChevronDown, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.item_media_info), style = MaterialTheme.typography.titleMedium, color = PleyColors.Muted)
            if (files.size > 1) {
                Spacer(Modifier.width(6.dp))
                Text(pluralStringResource(R.plurals.item_media_versions, files.size, files.size), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
            }
        }
        if (open) {
            files.forEach { file ->
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(top = 8.dp)
                        .background(PleyColors.Surface, PleyShape.Card)
                        .padding(16.dp),
                ) {
                    file.title?.let { Text(it.asString(), style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, modifier = Modifier.padding(bottom = 6.dp)) }
                    file.rows.forEach { row -> DetailRow(infoLabel(row.label), AnnotatedString(row.value)) }
                    if (file.streams.isNotEmpty()) {
                        Spacer(Modifier.height(10.dp))
                        Text(stringResource(R.string.item_info_streams), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
                        file.streams.forEach { stream ->
                            val kind = when (stream.kind) {
                                StreamKind.Video -> stringResource(R.string.item_stream_video)
                                StreamKind.Audio -> stringResource(R.string.item_stream_audio)
                                else -> stringResource(R.string.item_stream_subtitle)
                            }
                            Column(Modifier.padding(top = 8.dp)) {
                                Text(
                                    listOfNotNull(kind, stream.codec.takeIf { it.isNotEmpty() }, stream.language, stream.title).joinToString(" · "),
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = PleyColors.Fg,
                                )
                                if (stream.details.isNotEmpty()) {
                                    Text(
                                        stream.details.joinToString(" · ") { it.resolve(resources) },
                                        style = MaterialTheme.typography.bodySmall,
                                        color = PleyColors.Muted,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun infoLabel(label: InfoLabel): String = stringResource(
    when (label) {
        InfoLabel.File -> R.string.item_info_file
        InfoLabel.Container -> R.string.item_info_container
        InfoLabel.Size -> R.string.item_info_size
        InfoLabel.Duration -> R.string.item_info_duration
        InfoLabel.Bitrate -> R.string.item_info_bitrate
        InfoLabel.Video -> R.string.item_info_video
        InfoLabel.Audio -> R.string.item_info_audio
    },
)

@Composable
internal fun SectionTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg, modifier = Modifier.semantics { heading() })
}

/** The phone's episode still (design spec §4.4). */
private val EpisodeStillWidth = 128.dp

private val DetailLabelWidth = 120.dp
