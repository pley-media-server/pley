package app.pley.android.ui.phone.settings

import androidx.annotation.OptIn
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.media3.common.MediaLibraryInfo
import androidx.media3.common.util.UnstableApi
import app.pley.android.common.Language
import app.pley.android.data.FrameRateMatching
import app.pley.android.data.GridDensity
import app.pley.android.data.Settings
import app.pley.android.data.SubtitleBackground
import app.pley.android.data.SubtitleFont
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.screens.downloads.DownloadsViewModel
import app.pley.android.screens.settings.LanguageUiState
import app.pley.android.screens.settings.LanguageViewModel
import app.pley.android.screens.settings.SettingsUiState
import app.pley.android.screens.settings.SettingsViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.account.SwitchRow
import app.pley.android.ui.phone.account.versionOf
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.screens.R as ScreensR

/**
 * This phone's settings (design spec §4.8; P5-ACCOUNT): the account's
 * language, quality on Wi-Fi and on mobile data, matching the refresh rate
 * (only when seamless, or off), autoplay, the playback details overlay,
 * the subtitles' size and background, the grid's density, the private
 * sign-in tab, and About (with TMDB's notice). Every change is saved at
 * once.
 */
@Composable
internal fun SettingsScreen() {
    val model: SettingsViewModel = viewModel(factory = SettingsViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val downloads: DownloadsViewModel = viewModel(factory = DownloadsViewModel.factory)
    val languageModel: LanguageViewModel = viewModel(factory = LanguageViewModel.factory)
    val language by languageModel.state.collectAsStateWithLifecycle()
    var confirmDeleteAll by rememberSaveable { mutableStateOf(false) }
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    LaunchedEffect(model) { model.errors.collect(shell::report) }
    LaunchedEffect(languageModel) { languageModel.errors.collect(shell::report) }
    var sheet by rememberSaveable { mutableStateOf<Sheet?>(null) }
    val list = rememberLazyListState()
    val s = state.settings
    Column(Modifier.fillMaxSize().testTag(SETTINGS_PAGE)) {
        PleyTopAppBar(title = stringResource(R.string.account_settings), onBack = navigator::back, scrolled = { list.canScrollBackward })
        val metrics = LocalPhoneMetrics.current
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (language.available) {
                item(key = "language") {
                    SettingsGroup(null, Modifier.widthIn(max = MAX_WIDTH), footer = language.hint.asString()) {
                        SettingsRow(
                            stringResource(ScreensR.string.language_title),
                            value = language.value.asString(),
                            icon = PleyIcons.Globe,
                            enabled = !language.saving,
                            onClick = { sheet = Sheet.Language },
                        )
                    }
                }
            }
            item(key = "playback") {
                SettingsGroup(stringResource(R.string.settings_playback), Modifier.widthIn(max = MAX_WIDTH)) {
                    SettingsRow(
                        stringResource(R.string.settings_quality_home),
                        value = qualityLabel(s.qualityOnHomeNetwork),
                        icon = PleyIcons.Wifi,
                        onClick = { sheet = Sheet.QualityHome },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.settings_quality_mobile),
                        value = qualityLabel(s.qualityOnMobile),
                        icon = PleyIcons.Smartphone,
                        onClick = { sheet = Sheet.QualityMobile },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.settings_refresh_rate),
                        value = frameRateLabel(state.frameRate),
                        icon = PleyIcons.Monitor,
                        onClick = { sheet = Sheet.FrameRate },
                    )
                    RowDivider()
                    SwitchRow(
                        stringResource(R.string.settings_autoplay),
                        subtitle = stringResource(R.string.settings_autoplay_hint),
                        checked = s.autoplayNext,
                        onChange = model::setAutoplayNext,
                        icon = PleyIcons.SkipForward,
                    )
                    RowDivider()
                    SwitchRow(
                        stringResource(R.string.settings_playback_details),
                        subtitle = stringResource(R.string.settings_playback_details_hint),
                        checked = s.showPlaybackDetails,
                        onChange = model::setPlaybackDetails,
                        icon = PleyIcons.Info,
                    )
                }
            }
            if (downloads.available) {
                item(key = "downloads") {
                    SettingsGroup(
                        stringResource(R.string.downloads_settings),
                        Modifier.widthIn(max = MAX_WIDTH),
                        footer = stringResource(R.string.downloads_quality_hint),
                    ) {
                        SettingsRow(
                            stringResource(R.string.downloads_quality),
                            value = qualityLabel(s.downloadQuality),
                            icon = PleyIcons.Gauge,
                            onClick = { sheet = Sheet.DownloadQuality },
                        )
                        RowDivider()
                        SwitchRow(
                            stringResource(R.string.downloads_mobile_data),
                            subtitle = stringResource(R.string.downloads_mobile_data_value),
                            checked = s.downloadOnMobileData,
                            onChange = model::setDownloadOnMobileData,
                            icon = PleyIcons.Download,
                        )
                        RowDivider()
                        SettingsRow(
                            stringResource(R.string.downloads_delete_all),
                            icon = PleyIcons.Trash2,
                            danger = true,
                            onClick = { confirmDeleteAll = true },
                        )
                    }
                }
            }
            item(key = "subtitles") {
                SettingsGroup(stringResource(R.string.settings_subtitle_style), Modifier.widthIn(max = MAX_WIDTH), footer = stringResource(R.string.settings_subtitle_style_hint)) {
                    SettingsRow(
                        stringResource(R.string.settings_subtitle_size),
                        value = stringResource(sizeLabel(s.subtitleSize)),
                        icon = PleyIcons.Captions,
                        onClick = { sheet = Sheet.SubtitleSize },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.settings_subtitle_background),
                        value = stringResource(backgroundLabel(s.subtitleBackground)),
                        icon = PleyIcons.Square,
                        onClick = { sheet = Sheet.SubtitleBackground },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.settings_subtitle_font),
                        value = stringResource(fontLabel(s.subtitleFont)),
                        icon = PleyIcons.Pencil,
                        onClick = { sheet = Sheet.SubtitleFont },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.settings_subtitle_position),
                        value = stringResource(raiseLabel(s.subtitleRaise)),
                        icon = PleyIcons.ArrowUp,
                        onClick = { sheet = Sheet.SubtitleRaise },
                    )
                }
            }
            item(key = "display") {
                SettingsGroup(stringResource(R.string.settings_display), Modifier.widthIn(max = MAX_WIDTH)) {
                    SettingsRow(
                        stringResource(R.string.settings_grid_density),
                        value = stringResource(densityLabel(s.gridDensity)),
                        icon = PleyIcons.LayoutDashboard,
                        onClick = { sheet = Sheet.GridDensity },
                    )
                }
            }
            item(key = "privacy") {
                SettingsGroup(stringResource(R.string.settings_privacy), Modifier.widthIn(max = MAX_WIDTH)) {
                    SwitchRow(
                        stringResource(R.string.settings_private_tab),
                        subtitle = stringResource(R.string.settings_private_tab_hint),
                        checked = s.privateSignInTab,
                        onChange = model::setPrivateTab,
                        icon = PleyIcons.ShieldCheck,
                    )
                }
            }
            if (state.debuggable) {
                item(key = "advanced") {
                    SettingsGroup(stringResource(R.string.settings_advanced), Modifier.widthIn(max = MAX_WIDTH)) {
                        SwitchRow(
                            stringResource(R.string.settings_software_video),
                            subtitle = stringResource(R.string.settings_software_video_hint),
                            checked = s.allowSoftwareVideo,
                            onChange = model::setSoftwareVideo,
                            icon = PleyIcons.Cpu,
                        )
                    }
                }
            }
            item(key = "about") {
                // TMDB's terms ask every app that shows its data to say this where it says what it is.
                SettingsGroup(
                    stringResource(R.string.settings_about),
                    Modifier.widthIn(max = MAX_WIDTH),
                    footer = stringResource(ScreensR.string.requests_credit_about),
                ) {
                    val context = LocalContext.current
                    SettingsRow(
                        stringResource(R.string.settings_version),
                        value = versionOf(context),
                        icon = PleyIcons.Info,
                        chevron = false,
                        onClick = {},
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.settings_licences),
                        value = stringResource(R.string.settings_licences_value),
                        icon = PleyIcons.List,
                        onClick = { sheet = Sheet.Licences },
                    )
                }
            }
        }
    }
    SettingsSheet(sheet, state, model, language, languageModel::choose, onDismiss = { sheet = null })
    if (confirmDeleteAll) {
        ConfirmDialog(
            title = stringResource(R.string.downloads_delete_all_title),
            body = stringResource(R.string.downloads_delete_all_body),
            confirmLabel = stringResource(R.string.downloads_delete_all_confirm),
            onConfirm = {
                confirmDeleteAll = false
                downloads.removeAll()
            },
            onDismiss = { confirmDeleteAll = false },
            destructive = true,
            dismissLabel = stringResource(R.string.downloads_keep),
        )
    }
}

/** The choice being made, if any. */
private enum class Sheet {
    Language,
    QualityHome,
    QualityMobile,
    DownloadQuality,
    FrameRate,
    SubtitleSize,
    SubtitleBackground,
    SubtitleFont,
    SubtitleRaise,
    GridDensity,
    Licences,
}

@Composable
private fun SettingsSheet(
    sheet: Sheet?,
    state: SettingsUiState,
    model: SettingsViewModel,
    language: LanguageUiState,
    onLanguage: (Language?) -> Unit,
    onDismiss: () -> Unit,
) {
    val s = state.settings
    when (sheet) {
        Sheet.Language -> ChoiceSheet(
            title = stringResource(ScreensR.string.language_title),
            choices = language.choices.map { Choice(it, language.label(it).asString()) },
            selected = language.chosen,
            onPick = onLanguage,
            onDismiss = onDismiss,
        )
        Sheet.QualityHome, Sheet.QualityMobile -> {
            val mobile = sheet == Sheet.QualityMobile
            ChoiceSheet(
                title = stringResource(if (mobile) R.string.settings_quality_mobile else R.string.settings_quality_home),
                choices = SettingsViewModel.QUALITIES.map { Choice(it.id, qualityLabel(it.id), qualityHint(it.id)) },
                selected = SettingsViewModel.presetOf(if (mobile) s.qualityOnMobile else s.qualityOnHomeNetwork).id,
                onPick = { id -> model.setQuality(mobile, SettingsViewModel.presetOf(id)) },
                onDismiss = onDismiss,
            )
        }
        Sheet.DownloadQuality -> ChoiceSheet(
            title = stringResource(R.string.downloads_quality),
            // Up to 1080p: what a phone shows.
            choices = SettingsViewModel.DOWNLOAD_QUALITIES.map { Choice(it.id, qualityLabel(it.id)) },
            selected = SettingsViewModel.presetOf(s.downloadQuality).id,
            onPick = { id -> model.setDownloadQuality(SettingsViewModel.presetOf(id)) },
            onDismiss = onDismiss,
        )
        Sheet.FrameRate -> ChoiceSheet(
            title = stringResource(R.string.settings_refresh_rate),
            choices = state.frameRateChoices.map { Choice(it, frameRateLabel(it), frameRateHint(it)) },
            selected = state.frameRate,
            onPick = model::setFrameRate,
            onDismiss = onDismiss,
        )
        Sheet.SubtitleSize -> ChoiceSheet(
            title = stringResource(R.string.settings_subtitle_size),
            choices = Settings.SUBTITLE_SIZES.map { Choice(it, stringResource(sizeLabel(it))) },
            selected = s.subtitleSize,
            onPick = model::setSubtitleSize,
            onDismiss = onDismiss,
        )
        Sheet.SubtitleBackground -> ChoiceSheet(
            title = stringResource(R.string.settings_subtitle_background),
            choices = SubtitleBackground.entries.map { Choice(it, stringResource(backgroundLabel(it))) },
            selected = s.subtitleBackground,
            onPick = model::setSubtitleBackground,
            onDismiss = onDismiss,
        )
        Sheet.SubtitleFont -> ChoiceSheet(
            title = stringResource(R.string.settings_subtitle_font),
            choices = CaptionLook.fonts().map { Choice(it, stringResource(fontLabel(it))) },
            selected = s.subtitleFont,
            onPick = model::setSubtitleFont,
            onDismiss = onDismiss,
        )
        Sheet.SubtitleRaise -> ChoiceSheet(
            title = stringResource(R.string.settings_subtitle_position),
            choices = Settings.SUBTITLE_RAISES.map { Choice(it, stringResource(raiseLabel(it))) },
            selected = s.subtitleRaise,
            onPick = model::setSubtitleRaise,
            onDismiss = onDismiss,
        )
        Sheet.GridDensity -> ChoiceSheet(
            title = stringResource(R.string.settings_grid_density),
            choices = GridDensity.entries.map { Choice(it, stringResource(densityLabel(it))) },
            selected = s.gridDensity,
            onPick = model::setGridDensity,
            onDismiss = onDismiss,
        )
        Sheet.Licences -> PleySheet(onDismiss = onDismiss, title = stringResource(R.string.settings_licences)) {
            licences().forEach { (name, licence) ->
                SettingsRow(name, value = licence, chevron = false, onClick = {})
            }
        }
        null -> Unit
    }
}

/** "Original", "Automatic", "8 Mbps 1080p", as the player's menu says them. */
@Composable
internal fun qualityLabel(value: String): String {
    val preset = SettingsViewModel.presetOf(value)
    if (preset.original) return stringResource(R.string.settings_quality_original)
    if (preset.automatic) return stringResource(R.string.settings_quality_automatic)
    val lines = when (preset.maxHeight) {
        2160 -> "4K"
        null -> ""
        else -> "${preset.maxHeight}p"
    }
    val mbps = (preset.maxBitrateKbps ?: 0) / 1000
    return stringResource(R.string.settings_quality_preset, lines, mbps)
}

/** What Original and Automatic do, under their names; `null` for the transcodes, whose names say it. */
@Composable
private fun qualityHint(value: String): String? {
    val preset = SettingsViewModel.presetOf(value)
    return when {
        preset.original -> stringResource(R.string.settings_quality_original_hint)
        preset.automatic -> stringResource(R.string.settings_quality_automatic_hint)
        else -> null
    }
}

@Composable
private fun frameRateLabel(choice: FrameRateMatching): String = stringResource(
    when (choice) {
        FrameRateMatching.Off -> R.string.settings_refresh_off
        FrameRateMatching.Seamless -> R.string.settings_refresh_seamless
        FrameRateMatching.Auto -> R.string.settings_refresh_auto
        FrameRateMatching.Always -> R.string.settings_refresh_always
    },
)

@Composable
private fun frameRateHint(choice: FrameRateMatching): String = stringResource(
    when (choice) {
        FrameRateMatching.Off -> R.string.settings_refresh_off_hint
        else -> R.string.settings_refresh_seamless_hint
    },
)

private fun sizeLabel(size: Float): Int = when (Settings.SUBTITLE_SIZES.indexOf(size)) {
    0 -> R.string.settings_size_small
    2 -> R.string.settings_size_large
    3 -> R.string.settings_size_largest
    else -> R.string.settings_size_medium
}

private fun backgroundLabel(background: SubtitleBackground): Int = when (background) {
    SubtitleBackground.None -> R.string.settings_background_none
    SubtitleBackground.DarkBox -> R.string.settings_background_box
}

internal fun fontLabel(font: SubtitleFont): Int = when (font) {
    SubtitleFont.Sans -> R.string.settings_font_sans
    SubtitleFont.Serif -> R.string.settings_font_serif
    SubtitleFont.Monospace -> R.string.settings_font_monospace
    SubtitleFont.Casual -> R.string.settings_font_casual
}

internal fun raiseLabel(raise: Float): Int = when (Settings.SUBTITLE_RAISES.indexOf(raise)) {
    1 -> R.string.settings_position_raised
    2 -> R.string.settings_position_higher
    3 -> R.string.settings_position_highest
    else -> R.string.settings_position_bottom
}

private fun densityLabel(density: GridDensity): Int = when (density) {
    GridDensity.Small -> R.string.settings_density_small
    GridDensity.Medium -> R.string.settings_density_medium
    GridDensity.Large -> R.string.settings_density_large
}

/** The libraries in the app, with their licences. */
@OptIn(UnstableApi::class)
internal fun licences(): List<Pair<String, String>> = listOf(
    "AndroidX (Compose, Navigation, DataStore, Lifecycle…)" to "Apache-2.0",
    "Media3 ${MediaLibraryInfo.VERSION}" to "Apache-2.0",
    "OkHttp ${okhttp3.OkHttp.VERSION}" to "Apache-2.0",
    "Coil" to "Apache-2.0",
    "kotlinx.coroutines, kotlinx.serialization" to "Apache-2.0",
    "Lucide icons" to "ISC",
    "QR Code generator (Nayuki)" to "MIT",
)

private val MAX_WIDTH = 640.dp

internal const val SETTINGS_PAGE = "pley.settings.page"
