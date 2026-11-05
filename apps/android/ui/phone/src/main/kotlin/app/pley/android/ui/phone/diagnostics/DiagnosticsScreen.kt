package app.pley.android.ui.phone.diagnostics

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.perf.FrameRate
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.diagnostics.DiagnosticsViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SwitchRow
import app.pley.android.ui.phone.account.versionOf
import app.pley.android.ui.phone.kit.BusyLine
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlinx.coroutines.launch

/**
 * Diagnostics (P5-ACCOUNT): the device and its memory tier, the player's
 * buffers, then what the capability probe found (decoders and their
 * limits, HDR types, Dolby Vision profiles, passthrough, display modes,
 * quirks) and the report the server decides from, as JSON to copy.
 */
@Composable
internal fun DiagnosticsScreen() {
    val model: DiagnosticsViewModel = viewModel(factory = DiagnosticsViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val clipboard = LocalClipboard.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val list = rememberLazyListState()
    val copy: () -> Unit = {
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Pley caps", state.capsJson)))
            shell.toasts.show(resources.getString(R.string.diagnostics_copied))
        }
    }
    Column(Modifier.fillMaxSize().testTag(DIAGNOSTICS_PAGE)) {
        PleyTopAppBar(
            title = stringResource(R.string.account_diagnostics),
            onBack = navigator::back,
            scrolled = { list.canScrollBackward },
            actions = {
                if (state.capsJson.isNotEmpty()) PleyIconButton(PleyIcons.Copy, stringResource(R.string.diagnostics_copy), onClick = copy)
            },
        )
        val metrics = LocalPhoneMetrics.current
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            item(key = "device") {
                SettingsGroup(stringResource(R.string.diagnostics_device), Modifier.widthIn(max = MAX_WIDTH)) {
                    FactRow(stringResource(R.string.diagnostics_app), stringResource(R.string.diagnostics_app_value, versionOf(context)))
                    state.facts.forEach { fact ->
                        RowDivider(inset = false)
                        FactRow(fact.label.asString(), fact.value.asString())
                    }
                    RowDivider(inset = false)
                    val frames by FrameRate.on.collectAsStateWithLifecycle()
                    SwitchRow(
                        stringResource(R.string.diagnostics_show_frame_rate),
                        subtitle = stringResource(R.string.diagnostics_show_frame_rate_hint),
                        checked = frames,
                        onChange = { FrameRate.show(context, it) },
                        modifier = Modifier.testTag(DIAGNOSTICS_FRAME_RATE),
                    )
                }
            }
            item(key = "caps") {
                SettingsGroup(stringResource(R.string.diagnostics_plays), Modifier.widthIn(max = MAX_WIDTH)) {
                    when {
                        state.lines.isNotEmpty() -> state.lines.forEachIndexed { i, line ->
                            if (i > 0) RowDivider(inset = false)
                            Text(
                                line,
                                style = MaterialTheme.typography.bodyMedium,
                                color = PleyColors.Fg,
                                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                            )
                        }
                        state.probing -> BusyLine(stringResource(R.string.diagnostics_probing), Modifier.padding(16.dp))
                        else -> Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text(stringResource(R.string.diagnostics_probe_failed), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText)
                            TonalButton(stringResource(R.string.diagnostics_probe_again), onClick = model::probe, icon = PleyIcons.RefreshCw)
                        }
                    }
                }
            }
            if (state.capsJson.isNotEmpty()) {
                item(key = "json") {
                    Column(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth()) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                stringResource(R.string.diagnostics_report),
                                style = MaterialTheme.typography.labelLarge,
                                color = PleyColors.Muted,
                                modifier = Modifier.weight(1f).padding(start = 4.dp).semantics { heading() },
                            )
                            TonalButton(stringResource(R.string.diagnostics_copy), onClick = copy, icon = PleyIcons.Copy)
                        }
                        Spacer(Modifier.heightIn(min = 8.dp))
                        Box(
                            Modifier
                                .fillMaxWidth()
                                .clip(PleyShape.Card)
                                .background(PleyColors.Surface)
                                .horizontalScroll(rememberScrollState())
                                .padding(16.dp),
                        ) {
                            Text(
                                state.capsJson,
                                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace, fontSize = 12.sp, lineHeight = 17.sp),
                                color = PleyColors.Fg,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun FactRow(label: String, value: String) {
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.width(120.dp))
        Text(value, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg, modifier = Modifier.weight(1f))
    }
}

private val MAX_WIDTH = 720.dp

internal const val DIAGNOSTICS_PAGE = "pley.diagnostics.page"
internal const val DIAGNOSTICS_FRAME_RATE = "pley.diagnostics.frameRate"
