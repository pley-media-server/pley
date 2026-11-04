package app.pley.android.ui.phone.livetv

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.model.CastKind
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.PlayOnButton
import app.pley.android.ui.phone.remote.canPlayOn

/**
 * A programme's details (the web's ProgramModal), in a sheet: its
 * channel, picture, title, episode, day and times with On now, Live, New
 * and its rating, how far it is, what the guide says of it and its
 * categories; then Watch (now, or the channel) and the favourite, and
 * Watch on… under them while another device takes Live TV.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ProgramSheet(
    channel: LiveChannel,
    program: LiveProgram,
    origin: String,
    now: Long,
    onWatch: () -> Unit,
    onFavorite: () -> Unit,
    onDismiss: () -> Unit,
) {
    val clock = rememberClock()
    val airing = program.airingAt(now)
    PleySheet(onDismiss = onDismiss, modifier = Modifier.testTag(LIVE_TV_PROGRAM)) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                ChannelTile(channel, origin, Modifier.size(width = 56.dp, height = 34.dp))
                Spacer(Modifier.width(12.dp))
                Text(
                    listOfNotNull(channel.number, channel.name).joinToString("  "),
                    style = MaterialTheme.typography.titleSmall,
                    color = PleyColors.Muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            ArtworkImage.of(program.image, origin)?.let { image ->
                Spacer(Modifier.height(16.dp))
                Artwork(image, program.title, Modifier.fillMaxWidth().aspectRatio(16f / 9f).clip(PleyShape.Card))
            }
            Spacer(Modifier.height(16.dp))
            Text(program.title, style = MaterialTheme.typography.headlineSmall, color = PleyColors.Fg)
            val episode = listOfNotNull(program.episodeLabel, program.subtitle).joinToString(" · ")
            if (episode.isNotEmpty()) {
                Text(episode, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.padding(top = 2.dp))
            }
            Spacer(Modifier.height(12.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(
                    "${dayOf(program.start, now)} · ${spanOf(program, clock)}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                )
                if (airing) ProgramBadge(stringResource(R.string.livetv_on_now), live = true)
                if (program.isLive) ProgramBadge(stringResource(R.string.livetv_badge_live), live = true)
                if (program.isNew) ProgramBadge(stringResource(R.string.livetv_badge_new), live = false)
                program.rating?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.labelSmall,
                        color = PleyColors.Muted,
                        modifier = Modifier.border(1.dp, PleyColors.Outline, PleyShape.Badge).padding(horizontal = 5.dp, vertical = 1.dp),
                    )
                }
            }
            if (airing) {
                Spacer(Modifier.height(12.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ProgramProgress(program, now, Modifier.weight(1f))
                    Spacer(Modifier.width(10.dp))
                    Text(timeLeft(program, now), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
                }
            }
            Spacer(Modifier.height(16.dp))
            Text(
                program.description?.takeIf { it.isNotBlank() } ?: stringResource(R.string.livetv_no_description),
                style = MaterialTheme.typography.bodyMedium,
                color = if (program.description.isNullOrBlank()) PleyColors.Subtle else PleyColors.Fg,
            )
            if (program.categories.isNotEmpty()) {
                Spacer(Modifier.height(12.dp))
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    for (category in program.categories) {
                        Text(
                            category,
                            style = MaterialTheme.typography.labelMedium,
                            color = PleyColors.Muted,
                            modifier = Modifier.border(1.dp, PleyColors.Outline, PleyShape.Circle).padding(horizontal = 10.dp, vertical = 3.dp),
                        )
                    }
                }
            }
            if (program.stop <= now) {
                Text(stringResource(R.string.livetv_ended), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, modifier = Modifier.padding(top = 12.dp))
            }
            Spacer(Modifier.height(20.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                PrimaryButton(
                    stringResource(if (airing) R.string.livetv_watch_now else R.string.livetv_watch_channel),
                    onClick = onWatch,
                    icon = PleyIcons.Play,
                    modifier = Modifier.weight(1f),
                )
                TonalButton(
                    stringResource(if (channel.favorite) R.string.livetv_favorite_on else R.string.livetv_favorite_add),
                    onClick = onFavorite,
                    icon = if (channel.favorite) PleyIcons.Star else PleyIcons.StarOutline,
                    modifier = Modifier.height(48.dp),
                )
            }
            if (canPlayOn(CastKind.Live)) {
                Spacer(Modifier.height(12.dp))
                PlayOnButton(CastTarget.channel(channel.id), Modifier.fillMaxWidth(), labelled = true, onOpen = onDismiss)
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

internal const val LIVE_TV_PROGRAM = "pley.live.program"
