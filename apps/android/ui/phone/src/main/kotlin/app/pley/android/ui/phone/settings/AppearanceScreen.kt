package app.pley.android.ui.phone.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.R as DesignR
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.theme.Appearance
import app.pley.android.designsystem.theme.PleyAccent
import app.pley.android.designsystem.theme.PleyPalette
import app.pley.android.designsystem.theme.PleyTheme
import app.pley.android.designsystem.theme.ThemePreview
import app.pley.android.designsystem.theme.aboutRes
import app.pley.android.designsystem.theme.accentNameRes
import app.pley.android.designsystem.theme.nameRes
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass

/**
 * Settings → Appearance (docs/THEMES.md, "In every app"): this phone's
 * theme, the nine as small previews of Pley in their colors with their
 * names and captions, then the accent over it, Default (the theme's own)
 * first, as the web's picker (ThemeCard.tsx). A pick shows at once,
 * everywhere, and stays on this phone ([Appearance]).
 */
@Composable
internal fun AppearanceScreen() {
    val context = LocalContext.current
    val navigator = LocalPhoneNavigator.current
    val shown = Appearance.palette
    val metrics = LocalPhoneMetrics.current
    val columns = if (metrics.widthClass == WidthClass.Compact) 2 else 3
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize()) {
        PleyTopAppBar(title = stringResource(DesignR.string.appearance_title), onBack = navigator::back, scrolled = { list.canScrollBackward })
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().testTag(APPEARANCE_PAGE),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            item(key = "theme") {
                Heading(stringResource(DesignR.string.appearance_theme), stringResource(DesignR.string.appearance_theme_hint))
            }
            PleyTheme.entries.chunked(columns).forEachIndexed { row, themes ->
                item(key = "themes-$row") {
                    Row(
                        Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth().padding(bottom = 12.dp).selectableGroup(),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        for (theme in themes) {
                            ThemeCard(
                                theme,
                                // Each preview in the accent shown, as the page will be.
                                PleyPalette.of(theme, shown.pick),
                                picked = theme == shown.theme,
                                onPick = { Appearance.pick(context, theme) },
                                modifier = Modifier.weight(1f),
                            )
                        }
                        // A short last row keeps the cards' width.
                        repeat(columns - themes.size) { Spacer(Modifier.weight(1f)) }
                    }
                }
            }
            item(key = "accent") {
                Column(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth().padding(top = 16.dp)) {
                    Heading(stringResource(DesignR.string.appearance_accent), stringResource(DesignR.string.appearance_accent_hint))
                    Accents(shown, onPick = { Appearance.pickAccent(context, it) })
                }
            }
        }
    }
}

@Composable
private fun Heading(title: String, hint: String) {
    Column(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth().padding(start = 4.dp, end = 4.dp, bottom = 12.dp)) {
        Text(title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, modifier = Modifier.semantics { heading() })
        Spacer(Modifier.height(4.dp))
        Text(hint, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
    }
}

/** One theme: its preview, its name and caption; the one shown in the accent's outline, checked. */
@Composable
private fun ThemeCard(theme: PleyTheme, palette: PleyPalette, picked: Boolean, onPick: () -> Unit, modifier: Modifier = Modifier) {
    Box(modifier) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(PleyShape.Card)
                .background(if (picked) PleyColors.Accent.copy(alpha = 0.06f) else PleyColors.Surface)
                .border(1.dp, if (picked) PleyColors.Accent else PleyColors.Border, PleyShape.Card)
                .selectable(selected = picked, role = Role.RadioButton, onClick = onPick)
                .padding(8.dp)
                .testTag("$APPEARANCE_THEME.${theme.id}"),
        ) {
            ThemePreview(palette, Modifier.fillMaxWidth(), corner = PleyShape.ControlRadius)
            Spacer(Modifier.height(8.dp))
            Text(stringResource(theme.nameRes), style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, modifier = Modifier.padding(horizontal = 4.dp))
            Text(
                stringResource(theme.aboutRes),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Muted,
                minLines = 2,
                maxLines = 2,
                modifier = Modifier.padding(horizontal = 4.dp),
            )
        }
        if (picked) PickedMark(Modifier.align(Alignment.TopEnd).offset(x = 6.dp, y = (-6).dp))
    }
}

/** The check on the theme picked: the accent's, as the web's. */
@Composable
private fun PickedMark(modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(20.dp)
            .background(PleyColors.Accent, CircleShape)
            .border(2.dp, PleyColors.Surface, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(PleyIcons.Check, contentDescription = null, size = 12.dp, tint = PleyColors.OnAccent)
    }
}

/** The accents, Default first: each a swatch in the color the page would have, and its name. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Accents(shown: PleyPalette, onPick: (PleyAccent?) -> Unit) {
    FlowRow(
        Modifier.fillMaxWidth().selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        for (accent in listOf(null) + PleyAccent.entries) {
            val picked = accent == shown.pick
            // The swatch: the theme shown, in that accent.
            val swatch = PleyPalette.of(shown.theme, accent)
            Row(
                Modifier
                    .heightIn(min = 48.dp)
                    .clip(PleyShape.Pill)
                    .background(if (picked) PleyColors.Accent.copy(alpha = 0.06f) else PleyColors.Surface)
                    .border(1.dp, if (picked) PleyColors.Accent else PleyColors.Border, PleyShape.Pill)
                    .selectable(selected = picked, role = Role.RadioButton, onClick = { onPick(accent) })
                    .padding(start = 10.dp, end = 14.dp)
                    .testTag("$APPEARANCE_ACCENT.${accent?.id ?: "own"}"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier
                        .size(20.dp)
                        .background(swatch.accent, CircleShape)
                        .border(1.dp, PleyColors.Fg.copy(alpha = 0.2f), CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    if (picked) Icon(PleyIcons.Check, contentDescription = null, size = 12.dp, tint = swatch.accentFg)
                }
                Spacer(Modifier.width(8.dp))
                Text(
                    stringResource(accentNameRes(accent)),
                    style = MaterialTheme.typography.labelLarge,
                    color = if (picked) PleyColors.Fg else PleyColors.Muted,
                )
            }
        }
    }
}

/** The theme's value in Settings: its name, and the accent's when one is picked over it. */
@Composable
internal fun appearanceValue(palette: PleyPalette): String {
    val theme = stringResource(palette.theme.nameRes)
    return palette.pick?.let { theme + " · " + stringResource(accentNameRes(it)) } ?: theme
}

private val MAX_WIDTH = 720.dp

internal const val APPEARANCE_PAGE = "pley.phone.appearance"
internal const val APPEARANCE_THEME = "pley.phone.appearance.theme"
internal const val APPEARANCE_ACCENT = "pley.phone.appearance.accent"
