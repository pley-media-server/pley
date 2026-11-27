package app.pley.android.ui.tv.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
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
import app.pley.android.ui.tv.item.focusRing
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus

/**
 * Settings → Appearance (docs/THEMES.md, "In every app"): this TV's theme,
 * the nine as small previews of Pley in their colors, three to a row, each
 * with its name and caption, then the accent over it, Default (the theme's
 * own) first. The current pick is marked, and focus starts on its theme. A
 * pick shows at once, everywhere, and stays on this TV ([Appearance]).
 */
@Composable
internal fun TvAppearanceScreen() {
    val context = LocalContext.current
    val shown = Appearance.palette
    LazyColumn(
        Modifier.fillMaxSize().heldKeyStaysIn(HeldEnds.UpDown).focusGroup().testTag(TV_APPEARANCE_PAGE),
        contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical, bottom = TvSafeArea.Vertical + 24.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        item(key = "title") {
            Column {
                BasicText(stringResource(DesignR.string.appearance_title), style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg))
                Spacer(Modifier.height(16.dp))
                BasicText(stringResource(DesignR.string.appearance_theme), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
                Spacer(Modifier.height(4.dp))
                BasicText(stringResource(DesignR.string.appearance_theme_hint_tv), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
            }
        }
        PleyTheme.entries.chunked(THEMES_PER_ROW).forEachIndexed { row, themes ->
            item(key = "themes-$row") {
                Row(horizontalArrangement = Arrangement.spacedBy(CARD_GAP)) {
                    for (theme in themes) {
                        ThemeCard(
                            theme = theme,
                            // Each preview in the accent shown, as the page will be.
                            palette = PleyPalette.of(theme, shown.pick),
                            picked = theme == shown.theme,
                            onPick = { Appearance.pick(context, theme) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                    // A short last row keeps the cards' width.
                    repeat(THEMES_PER_ROW - themes.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
        item(key = "accent") {
            Column {
                Spacer(Modifier.height(4.dp))
                BasicText(stringResource(DesignR.string.appearance_accent), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
                Spacer(Modifier.height(4.dp))
                BasicText(stringResource(DesignR.string.appearance_accent_hint), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
                Spacer(Modifier.height(16.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    for (accent in listOf(null) + PleyAccent.entries) {
                        AccentChip(
                            accent = accent,
                            // The swatch is the color the page will have: the theme shown, in that accent.
                            palette = PleyPalette.of(shown.theme, accent),
                            picked = accent == shown.pick,
                            onPick = { Appearance.pickAccent(context, accent) },
                        )
                    }
                }
            }
        }
    }
}

/** One theme: its preview, marked when it's the one shown, with its name and caption under it. */
@Composable
private fun ThemeCard(theme: PleyTheme, palette: PleyPalette, picked: Boolean, onPick: () -> Unit, modifier: Modifier = Modifier) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val ring = PleyColors.AccentBright
    Column(
        modifier
            .initialFocus(enabled = picked)
            .semantics { selected = picked }
            .clickable(interactionSource = interactions, indication = null, role = Role.RadioButton, onClick = onPick)
            .testTag("$TV_APPEARANCE_THEME.${theme.id}"),
    ) {
        Box(Modifier.fillMaxWidth().focusRing(PleyShape.CardRadius, ring) { focused }) {
            ThemePreview(
                palette,
                corner = PleyShape.CardRadius,
                modifier = Modifier
                    .fillMaxWidth()
                    .then(if (picked) Modifier.border(2.dp, PleyColors.Accent, RoundedCornerShape(PleyShape.CardRadius)) else Modifier),
            )
            if (picked) PickedMark(Modifier.align(Alignment.TopEnd).offset(x = 6.dp, y = (-6).dp))
        }
        Spacer(Modifier.height(10.dp))
        BasicText(stringResource(theme.nameRes), style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
        BasicText(stringResource(theme.aboutRes), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** The check on what is picked: the accent's, as the web's. */
@Composable
private fun PickedMark(modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(22.dp)
            .background(PleyColors.Accent, CircleShape)
            .border(2.dp, PleyColors.Bg, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(PleyIcons.Check, contentDescription = null, size = 14.dp, tint = PleyColors.OnAccent)
    }
}

/** One accent: its swatch, in the color the page would have, and its name; the one shown is outlined in the accent and checked. */
@Composable
private fun AccentChip(accent: PleyAccent?, palette: PleyPalette, picked: Boolean, onPick: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val ring = PleyColors.AccentBright
    Row(
        Modifier
            .focusRing(PleyShape.ControlRadius, ring) { focused }
            .semantics { selected = picked }
            .clickable(interactionSource = interactions, indication = null, role = Role.RadioButton, onClick = onPick)
            .background(if (picked) PleyColors.Accent.copy(alpha = 0.08f) else PleyColors.Surface, PleyShape.Pill)
            .border(1.dp, if (picked) PleyColors.Accent else PleyColors.Border, PleyShape.Pill)
            .padding(start = 8.dp, end = 14.dp, top = 8.dp, bottom = 8.dp)
            .testTag("$TV_APPEARANCE_ACCENT.${accent?.id ?: "own"}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(22.dp)
                .background(palette.accent, CircleShape)
                .border(1.dp, PleyColors.Fg.copy(alpha = 0.2f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (picked) Icon(PleyIcons.Check, contentDescription = null, size = 14.dp, tint = palette.accentFg)
        }
        Spacer(Modifier.width(8.dp))
        BasicText(stringResource(accentNameRes(accent)), style = PleyType.Tv.Label.copy(color = if (picked) PleyColors.Fg else PleyColors.Muted), maxLines = 1)
    }
}

/** The theme's value in Settings: its name, and the accent's when one is picked over it. */
@Composable
internal fun appearanceValue(palette: PleyPalette): String {
    val theme = stringResource(palette.theme.nameRes)
    return palette.pick?.let { theme + " · " + stringResource(accentNameRes(it)) } ?: theme
}

private const val THEMES_PER_ROW = 3
private val CARD_GAP = 24.dp

internal const val TV_APPEARANCE_PAGE = "pley.tv.appearance"
internal const val TV_APPEARANCE_THEME = "pley.tv.appearance.theme"
internal const val TV_APPEARANCE_ACCENT = "pley.tv.appearance.accent"
