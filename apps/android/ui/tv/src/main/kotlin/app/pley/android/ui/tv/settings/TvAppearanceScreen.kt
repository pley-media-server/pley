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
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
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
import app.pley.android.ui.tv.kit.initialFocus

/**
 * Settings → Appearance (docs/THEMES.md, "In every app"; the same page on
 * every TV app): this TV's theme, the nine as small previews of Pley in
 * their colors, five to a row, each with its name and caption, then the
 * accents over it, Default (the theme's own) first, all on one screen.
 * What is in use is marked; focus starts on its theme, or on its accent
 * when the page opens from Settings' Accent row ([accent]), and Up and
 * Down between the themes and the accents land on what is in use. A pick
 * shows at once, everywhere, and stays on this TV for the profile
 * ([Appearance]).
 */
@Composable
internal fun TvAppearanceScreen(accent: Boolean = false) {
    val context = LocalContext.current
    val shown = Appearance.palette
    val themes = remember { PleyTheme.entries.associateWith { FocusRequester() } }
    val accents = remember { (listOf(null) + PleyAccent.entries).associateWith { FocusRequester() } }
    val themeInUse = themes.getValue(shown.theme)
    val accentInUse = accents.getValue(shown.pick)
    Column(
        Modifier
            .fillMaxSize()
            .padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical)
            .focusGroup()
            .testTag(TV_APPEARANCE_PAGE),
    ) {
        BasicText(stringResource(DesignR.string.appearance_title), style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg))
        Spacer(Modifier.height(12.dp))
        SectionLine(stringResource(DesignR.string.appearance_theme), stringResource(DesignR.string.appearance_theme_hint_tv))
        Spacer(Modifier.height(10.dp))
        Column(verticalArrangement = Arrangement.spacedBy(GAP)) {
            PleyTheme.entries.chunked(THEMES_PER_ROW).forEachIndexed { row, line ->
                Row(horizontalArrangement = Arrangement.spacedBy(GAP)) {
                    line.forEachIndexed { column, theme ->
                        // Down from a theme with none under it goes to the accent in use.
                        val last = row == 1 || column >= PleyTheme.entries.size - THEMES_PER_ROW
                        ThemeCard(
                            theme = theme,
                            // Each preview in the accent shown, as the page will be.
                            palette = PleyPalette.of(theme, shown.pick),
                            picked = theme == shown.theme,
                            initial = !accent && theme == shown.theme,
                            onPick = { Appearance.pick(context, theme) },
                            modifier = Modifier
                                .weight(1f)
                                .focusRequester(themes.getValue(theme))
                                .then(if (last) Modifier.focusProperties { down = accentInUse } else Modifier),
                        )
                    }
                    // The short last row keeps the cards' width.
                    repeat(THEMES_PER_ROW - line.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
        Spacer(Modifier.height(20.dp))
        SectionLine(stringResource(DesignR.string.appearance_accent), stringResource(DesignR.string.appearance_accent_hint))
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            for ((choice, requester) in accents) {
                AccentChip(
                    accent = choice,
                    // The swatch is the color the page would have: the theme shown, in that accent.
                    palette = PleyPalette.of(shown.theme, choice),
                    picked = choice == shown.pick,
                    initial = accent && choice == shown.pick,
                    onPick = { Appearance.pickAccent(context, choice) },
                    modifier = Modifier.focusRequester(requester).focusProperties { up = themeInUse },
                )
            }
        }
    }
}

/** A section's label, and its hint after it on the same line. */
@Composable
private fun SectionLine(label: String, hint: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        BasicText(label, style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Fg), maxLines = 1)
        Spacer(Modifier.width(16.dp))
        BasicText(hint, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** One theme: its preview, its name and caption; the one in use outlined in the accent (the ring alone while focused), checked. */
@Composable
private fun ThemeCard(theme: PleyTheme, palette: PleyPalette, picked: Boolean, initial: Boolean, onPick: () -> Unit, modifier: Modifier = Modifier) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val ring = PleyColors.AccentBright
    val shape = RoundedCornerShape(PleyShape.CardRadius)
    Box(
        modifier
            .initialFocus(enabled = initial)
            .focusRing(PleyShape.CardRadius, ring) { focused }
            .semantics { selected = picked }
            .clickable(interactionSource = interactions, indication = null, role = Role.RadioButton, onClick = onPick)
            .background(if (picked) PleyColors.Accent.copy(alpha = 0.06f) else Color.Transparent, shape)
            // One outline: while the ring shows, the card's own border goes.
            .then(if (focused) Modifier else Modifier.border(if (picked) 2.dp else 1.dp, if (picked) PleyColors.Accent else PleyColors.Border, shape))
            .testTag("$TV_APPEARANCE_THEME.${theme.id}"),
    ) {
        Column(Modifier.padding(5.dp)) {
            ThemePreview(palette, Modifier.fillMaxWidth(), corner = PleyShape.FieldRadius)
            Spacer(Modifier.height(6.dp))
            Column(Modifier.padding(horizontal = 3.dp)) {
                BasicText(stringResource(theme.nameRes), style = PleyType.Tv.Body.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
                BasicText(
                    stringResource(theme.aboutRes),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                    minLines = 2,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (picked) PickedMark(Modifier.align(Alignment.TopEnd).padding(top = 10.dp, end = 10.dp))
    }
}

/** The check on the theme in use: the accent's, as the web's. */
@Composable
private fun PickedMark(modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(20.dp)
            .background(PleyColors.Accent, CircleShape)
            .border(1.5.dp, PleyColors.Surface, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(PleyIcons.Check, contentDescription = null, size = 12.dp, tint = PleyColors.OnAccent)
    }
}

/**
 * One accent: its swatch, in the color it gives the theme shown, and its
 * name. The one in use is filled with the accent, a check in place of its
 * swatch.
 */
@Composable
private fun AccentChip(accent: PleyAccent?, palette: PleyPalette, picked: Boolean, initial: Boolean, onPick: () -> Unit, modifier: Modifier = Modifier) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val ring = PleyColors.AccentBright
    val words = if (picked) PleyColors.OnAccent else PleyColors.Fg
    Row(
        modifier
            .initialFocus(enabled = initial)
            .focusRing(PleyShape.ControlRadius, ring) { focused }
            .semantics { selected = picked }
            .clickable(interactionSource = interactions, indication = null, role = Role.RadioButton, onClick = onPick)
            .background(if (picked) PleyColors.Accent else Color.Transparent, PleyShape.Pill)
            .then(if (focused) Modifier else Modifier.border(1.dp, if (picked) PleyColors.Accent else PleyColors.Border, PleyShape.Pill))
            .padding(start = 10.dp, end = 14.dp, top = 9.dp, bottom = 9.dp)
            .testTag("$TV_APPEARANCE_ACCENT.${accent?.id ?: "own"}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (picked) {
            Box(Modifier.size(20.dp).border(1.5.dp, words.copy(alpha = 0.6f), CircleShape), contentAlignment = Alignment.Center) {
                Icon(PleyIcons.Check, contentDescription = null, size = 12.dp, tint = words)
            }
        } else {
            Box(Modifier.size(20.dp).background(palette.accent, CircleShape).border(1.dp, PleyColors.Fg.copy(alpha = 0.2f), CircleShape))
        }
        Spacer(Modifier.width(8.dp))
        BasicText(stringResource(accentNameRes(accent)), style = PleyType.Tv.BodyLarge.copy(color = words), maxLines = 1)
    }
}

private const val THEMES_PER_ROW = 5
private val GAP = 15.dp

internal const val TV_APPEARANCE_PAGE = "pley.tv.appearance"
internal const val TV_APPEARANCE_THEME = "pley.tv.appearance.theme"
internal const val TV_APPEARANCE_ACCENT = "pley.tv.appearance.accent"
