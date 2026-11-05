package app.pley.android.ui.tv.item

import android.content.res.Resources
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.resolve
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.text.asString
import app.pley.android.model.StreamKind
import app.pley.android.screens.item.FileInfo
import app.pley.android.screens.item.InfoLabel
import app.pley.android.ui.tv.R

/**
 * Media info at the end of an item page: for each version, the file and
 * every stream in full (codec, profile and level, size, frame rate, bit
 * depth, HDR and Dolby Vision, colour, channels, languages, flags), as text
 * in two columns of blocks. Nothing here takes focus.
 */
@Composable
internal fun TvMediaInfo(files: List<FileInfo>, modifier: Modifier = Modifier) {
    val resources = LocalResources.current
    Column(modifier.testTag(TAG_MEDIA_INFO), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        files.forEach { file ->
            val blocks = remember(file, resources) { blocksOf(resources, file) }
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(PleyColors.Surface2.copy(alpha = 0.6f), PleyShape.Card)
                    .padding(16.dp),
            ) {
                file.title?.let { BasicText(it.asString(), style = PleyType.Tv.Label.copy(color = PleyColors.Fg), modifier = Modifier.padding(bottom = 10.dp)) }
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(BlockGap),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                    maxItemsInEachRow = 2,
                ) {
                    blocks.forEach { FactBlock(it, Modifier.width(BlockWidth)) }
                }
            }
        }
    }
}

/** A block's title, then its facts: muted labels, values beside them. */
@Composable
internal fun FactBlock(block: MediaFacts, modifier: Modifier = Modifier, labelWidth: Dp = LabelWidth) {
    Column(modifier) {
        BasicText(block.title, style = PleyType.Tv.Label.copy(color = PleyColors.Fg), maxLines = 1, modifier = Modifier.padding(bottom = 4.dp))
        block.facts.forEach { FactLine(it, labelWidth) }
    }
}

/** One fact: the label in a column of its own, the value wrapping beside it. */
@Composable
internal fun FactLine(fact: MediaFact, labelWidth: Dp = LabelWidth) {
    Row(Modifier.padding(vertical = 2.dp)) {
        BasicText(fact.label, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(labelWidth))
        BasicText(fact.value, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), maxLines = 3, overflow = TextOverflow.Ellipsis)
    }
}

/** The file's facts, then each stream's; a page made without the file falls back to its short rows. */
private fun blocksOf(r: Resources, info: FileInfo): List<MediaFacts> {
    val file = info.file
    if (file != null) return listOf(MediaFacts(r.getString(R.string.tv_readout_file), MediaFactText.file(r, file))) + MediaFactText.streams(r, file)
    val rows = info.rows.map { MediaFact(r.getString(labelOf(it.label)), it.value) }
    val streams = info.streams.map { s ->
        val kind = r.getString(
            when (s.kind) {
                StreamKind.Video -> R.string.tv_readout_video
                StreamKind.Audio -> R.string.tv_readout_audio
                else -> R.string.tv_readout_subtitles
            },
        )
        val details = s.details.joinToString(" · ") { it.resolve(r) }
        MediaFacts(listOfNotNull(kind, s.language, s.title).joinToString(" · "), listOf(MediaFact(r.getString(R.string.tv_readout_codec), s.codec), MediaFact("", details)))
    }
    return listOf(MediaFacts(r.getString(R.string.tv_readout_file), rows)) + streams
}

private fun labelOf(label: InfoLabel): Int = when (label) {
    InfoLabel.File -> R.string.tv_readout_name
    InfoLabel.Container -> R.string.tv_readout_container
    InfoLabel.Size -> R.string.tv_readout_size
    InfoLabel.Duration -> R.string.tv_readout_duration
    InfoLabel.Bitrate -> R.string.tv_readout_overall_bitrate
    InfoLabel.Video -> R.string.tv_readout_video
    InfoLabel.Audio -> R.string.tv_readout_audio
}

/** Two blocks side by side in the details' width. */
private val BlockWidth = 300.dp
private val BlockGap = 24.dp
private val LabelWidth = 104.dp

internal const val TAG_MEDIA_INFO = "pley.tv.item.mediaInfo"
