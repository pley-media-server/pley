package app.pley.android.ui.tv.item

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.CreditRole
import app.pley.android.screens.item.FileInfo
import app.pley.android.screens.item.ItemPage
import app.pley.android.ui.tv.R

/**
 * Who made it and the facts (design spec §4.3), under the cast, where
 * they're read beside the people: the creators, directors and writers,
 * the original title, the release, the studio or network and the plays,
 * each a small label over its value, four to a line. Two lines at most
 * fit under the cast row while it has focus. Never takes focus.
 */
@Composable
internal fun TvFacts(page: ItemPage, modifier: Modifier = Modifier) {
    val details = page.details
    val facts = buildList {
        details.crew.forEach { line ->
            val label = when (line.role) {
                CreditRole.Creator -> stringResource(R.string.tv_item_created_by)
                CreditRole.Director -> stringResource(if (line.plural) R.string.tv_item_directors else R.string.tv_item_director)
                else -> stringResource(if (line.plural) R.string.tv_item_writers else R.string.tv_item_writer)
            }
            val more = if (line.more > 0) " " + pluralStringResource(R.plurals.tv_item_and_more, line.more, line.more) else ""
            add(label to line.people.joinToString(", ") { it.name } + more)
        }
        details.originalTitle?.let { add(stringResource(R.string.tv_item_original_title) to it) }
        details.released?.let { add(stringResource(R.string.tv_item_released) to it) }
        details.studio?.let { add(stringResource(if (details.network) R.string.tv_item_network else R.string.tv_item_studio) to it) }
        details.plays?.let { add(stringResource(R.string.tv_item_plays) to it.toString()) }
    }
    if (facts.isEmpty()) return
    Column(
        modifier
            .padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal)
            .semantics(mergeDescendants = true) {}
            .testTag(TAG_FACTS),
        verticalArrangement = Arrangement.spacedBy(FactLineGap),
    ) {
        facts.chunked(FactsPerLine).forEach { line ->
            Row(horizontalArrangement = Arrangement.spacedBy(FactGap)) {
                line.forEach { (label, value) -> Fact(label, value) }
            }
        }
    }
}

/**
 * Details at the end of the page (design spec §4.3): one row per version
 * in its long form ("4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 ·
 * 62.0 GB", the one that plays ticked; Select plays another), and the
 * media info of each file, closed until Select opens it. Who made it and
 * the facts are under the cast ([TvFacts]).
 */
@Composable
internal fun TvDetails(page: ItemPage, onChooseVersion: (Long) -> Unit, modifier: Modifier = Modifier) {
    val details = page.details
    Column(modifier.padding(start = PleyTv.Gutter, end = DetailsEnd)) {
        BasicText(stringResource(R.string.tv_item_details), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
        Spacer(Modifier.height(PleyTv.HeaderToTiles))
        if (details.versions.isNotEmpty()) {
            BasicText(stringResource(R.string.tv_item_versions), style = PleyType.Tv.Title.copy(color = PleyColors.Fg), modifier = Modifier.padding(bottom = 8.dp))
            details.versions.forEach { version ->
                PanelRow(version.label, onClick = { onChooseVersion(version.id) }, checked = version.selected)
            }
        }
        if (page.mediaInfo.isNotEmpty()) {
            Spacer(Modifier.height(20.dp))
            MediaInfoSection(page.mediaInfo, page.id)
        }
    }
}

/** Media info, closed until Select opens it, as on the web and phones: few want every stream. */
@Composable
internal fun MediaInfoSection(files: List<FileInfo>, itemId: Long) {
    var open by rememberSaveable(itemId) { mutableStateOf(false) }
    val state = stringResource(if (open) R.string.tv_item_media_info_open else R.string.tv_item_media_info_closed)
    Column {
        PanelRow(
            stringResource(R.string.tv_item_media_info),
            onClick = { open = !open },
            icon = if (open) PleyIcons.ChevronUp else PleyIcons.ChevronDown,
            supporting = stringResource(R.string.tv_item_media_info_hint),
            modifier = Modifier.testTag(TAG_MEDIA_INFO_TOGGLE).semantics { stateDescription = state },
        )
        if (open) {
            Spacer(Modifier.height(8.dp))
            TvMediaInfo(files)
        }
    }
}

/** A fact: its label in Subtle over its value, two lines at most. */
@Composable
private fun Fact(label: String, value: String) {
    Column(Modifier.width(FactWidth), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        BasicText(label, style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), maxLines = 1, overflow = TextOverflow.Ellipsis)
        BasicText(value, style = PleyType.Tv.Body.copy(color = PleyColors.Fg.copy(alpha = 0.9f)), maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** Details end well inside the safe area: long lines are easier to read narrower. */
private val DetailsEnd = 200.dp

/** Four facts to a line across the 816 dp between the gutter and the safe area. */
private const val FactsPerLine = 4
private val FactGap = 24.dp
private val FactWidth = 186.dp
private val FactLineGap = 12.dp

internal const val TAG_FACTS = "pley.tv.item.facts"

internal const val TAG_MEDIA_INFO_TOGGLE = "pley.tv.item.mediaInfoToggle"
