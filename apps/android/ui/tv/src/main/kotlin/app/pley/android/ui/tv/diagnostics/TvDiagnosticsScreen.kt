package app.pley.android.ui.tv.diagnostics

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.screens.diagnostics.DiagnosticsViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.settings.versionOf

/**
 * Diagnostics on TV: the device, its memory tier and the player's buffers,
 * then what the capability probe found, one line each (decoders and their
 * limits, HDR types, Dolby Vision profiles, the screen's modes,
 * passthrough, quirks). Every line takes focus, so the D-pad scrolls
 * through them; nothing on the page acts but Check again.
 */
@Composable
internal fun TvDiagnosticsScreen() {
    val model: DiagnosticsViewModel = viewModel(factory = DiagnosticsViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val device = listOf(stringResource(R.string.tv_diagnostics_app) to stringResource(R.string.tv_diagnostics_app_value, versionOf(context))) +
        state.device.map { it.label to it.value }
    LazyColumn(
        Modifier.fillMaxSize().heldKeyStaysIn(HeldEnds.UpDown).focusGroup().testTag(TV_DIAGNOSTICS_PAGE),
        contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical, bottom = 160.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        item(key = "title") {
            Column {
                BasicText(stringResource(R.string.tv_diagnostics_title), style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg))
                Spacer(Modifier.height(20.dp))
                BasicText(stringResource(R.string.tv_diagnostics_device), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
                Spacer(Modifier.height(8.dp))
            }
        }
        itemsIndexed(device, key = { i, _ -> "device-$i" }) { i, (label, value) ->
            Line(label, value, initial = i == 0)
        }
        item(key = "plays") {
            Column {
                Spacer(Modifier.height(20.dp))
                BasicText(stringResource(R.string.tv_diagnostics_plays), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
                Spacer(Modifier.height(8.dp))
            }
        }
        when {
            state.lines.isNotEmpty() -> itemsIndexed(state.lines, key = { i, _ -> "line-$i" }) { _, line -> Line(null, line) }
            state.probing -> item(key = "probing") {
                BasicText(stringResource(R.string.tv_diagnostics_probing), style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = Modifier.padding(start = 16.dp))
            }
            else -> item(key = "failed") {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    BasicText(stringResource(R.string.tv_diagnostics_probe_failed), style = PleyType.Tv.Body.copy(color = PleyColors.DangerText), modifier = Modifier.padding(start = 16.dp))
                    TvPillButton(stringResource(R.string.tv_diagnostics_probe_again), onClick = model::probe)
                }
            }
        }
    }
}

/** A line of facts; it takes focus (the accent fill, as every row), so the list scrolls with the D-pad. */
@Composable
private fun Line(label: String?, value: String, initial: Boolean = false) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 36.dp)
            .initialFocus(enabled = initial)
            .focusable(interactionSource = interactions)
            .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Field)
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        if (label != null) {
            BasicText(label, style = PleyType.Tv.Body.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Muted), modifier = Modifier.width(180.dp))
        }
        BasicText(value, style = PleyType.Tv.Body.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Fg), modifier = Modifier.weight(1f))
    }
}

internal const val TV_DIAGNOSTICS_PAGE = "pley.tv.diagnostics"
