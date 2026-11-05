package app.pley.android.ui.tv.settings

import androidx.annotation.OptIn
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.media3.common.MediaLibraryInfo
import androidx.media3.common.util.UnstableApi
import app.pley.android.auth.Needs
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.common.perf.FrameRate
import app.pley.android.data.FrameRateMatching
import app.pley.android.data.Passthrough
import app.pley.android.data.Settings
import app.pley.android.data.SubtitleBackground
import app.pley.android.data.SubtitleFont
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.SubtitleMode
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.account.MeEvent
import app.pley.android.screens.account.MeUiState
import app.pley.android.screens.account.MeViewModel
import app.pley.android.screens.account.SUBTITLE_MODES
import app.pley.android.screens.account.TrackLanguages
import app.pley.android.screens.diagnostics.DiagnosticsViewModel
import app.pley.android.screens.onboarding.ProfileTile
import app.pley.android.screens.settings.LanguageUiState
import app.pley.android.screens.settings.LanguageViewModel
import app.pley.android.screens.settings.SettingsUiState
import app.pley.android.screens.settings.SettingsViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.onboarding.ProfileAvatar
import app.pley.android.ui.tv.onboarding.TvDialog
import app.pley.android.ui.tv.shell.LocalTvNavigator

/** The TV settings' categories, top to bottom (design spec §4.8). */
internal enum class Category(val icon: PleyIcon) {
    Profile(PleyIcons.UserRound),
    Playback(PleyIcons.Play),
    Audio(PleyIcons.AudioLines),
    Display(PleyIcons.Captions),
    Server(PleyIcons.Server),
    Diagnostics(PleyIcons.Activity),
    About(PleyIcons.Info),
    SignOut(PleyIcons.LogOut),
}

/** The choice whose side panel is open. */
private sealed interface Picking {
    data object Language : Picking
    data object AudioLanguage : Picking
    data object SubtitleMode : Picking
    data object SubtitleLanguage : Picking
    data object AlwaysStartAs : Picking
    data object Quality : Picking
    data object FrameRate : Picking
    data class PassthroughOf(val format: String) : Picking
    data object SubtitleSize : Picking
    data object SubtitleBackground : Picking
    data object SubtitleFont : Picking
    data object SubtitleRaise : Picking
}

/**
 * The TV's settings, in two panes (design spec §4.8): the categories on
 * the left (Profile, Playback, Audio, Display, Server, Diagnostics,
 * About, Sign out), the focused category's rows on the right. Focus on a
 * category shows its rows; Right or Select goes into them. Choices open a
 * side panel; switches toggle on Select. Sign out asks first, and "Manage
 * your account on the web" shows the address and a QR code, since a TV
 * opens no browser.
 *
 * Me opens it on Profile.
 */
@Composable
internal fun TvSettingsScreen(start: Category = Category.Profile) {
    val me: MeViewModel = viewModel(factory = MeViewModel.factory)
    val settingsModel: SettingsViewModel = viewModel(factory = SettingsViewModel.factory)
    val languageModel: LanguageViewModel = viewModel(factory = LanguageViewModel.factory)
    val meState by me.state.collectAsStateWithLifecycle()
    val settings by settingsModel.state.collectAsStateWithLifecycle()
    val language by languageModel.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val memory = rememberFocusMemory()
    var shown by rememberSaveable { mutableStateOf(start) }
    var picking by remember { mutableStateOf<Picking?>(null) }
    var lastPicking by remember { mutableStateOf<Picking?>(null) }
    var web by rememberSaveable { mutableStateOf<String?>(null) }
    var confirmSignOut by rememberSaveable { mutableStateOf(false) }
    var message by remember { mutableStateOf<UiText?>(null) }
    val rows = remember { HashMap<String, FocusRequester>() }
    fun requester(key: String) = rows.getOrPut(key) { FocusRequester() }
    // Focus goes back to Sign out once its dialog closes without signing out.
    var askedSignOut by remember { mutableStateOf(false) }
    LaunchedEffect(confirmSignOut) {
        if (confirmSignOut) {
            askedSignOut = true
        } else if (askedSignOut) {
            askedSignOut = false
            androidx.compose.runtime.withFrameNanos { }
            runCatching { requester("category-${Category.SignOut.name}").requestFocus() }
        }
    }
    LaunchedEffect(me) {
        me.events.collect { event ->
            when (event) {
                is MeEvent.Saved -> message = UiText.res(R.string.tv_account_saved)
                MeEvent.ReadOnly -> message = UiText.res(R.string.tv_account_defaults_read_only)
                is MeEvent.Failed -> message = errorSentence(event.error)
                is MeEvent.SignedOut -> navigator.restart(event.server?.let { PleyKey.Link(it) } ?: PleyKey.Servers)
            }
        }
    }
    LaunchedEffect(languageModel) { languageModel.errors.collect { message = errorSentence(it) } }
    LaunchedEffect(message) {
        if (message != null) {
            kotlinx.coroutines.delay(MESSAGE_MS)
            message = null
        }
    }

    val pick: (Picking) -> Unit = {
        picking = it
        lastPicking = it
    }

    Box(Modifier.fillMaxSize().testTag(TV_SETTINGS_PAGE)) {
        Column(Modifier.fillMaxSize().padding(top = TvSafeArea.Vertical)) {
            BasicText(
                stringResource(R.string.tv_settings_title),
                style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                modifier = Modifier.padding(start = PleyTv.Gutter),
            )
            Spacer(Modifier.height(20.dp))
            Row(Modifier.fillMaxSize()) {
                // Categories: x = 96, 256 wide.
                Column(
                    Modifier
                        .padding(start = PleyTv.Gutter)
                        .width(CATEGORIES_WIDTH)
                        .fillMaxHeight()
                        // Right goes into the category's rows at their first, whatever is beside it.
                        .onPreviewKeyEvent { event ->
                            if (event.type == KeyEventType.KeyDown && event.key == Key.DirectionRight) {
                                rows["first-${shown.name}"]?.let { runCatching { it.requestFocus() }.isSuccess } ?: false
                            } else {
                                false
                            }
                        }
                        .heldKeyStaysIn(HeldEnds.UpDown)
                        .focusRestorer()
                        .focusGroup(),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Category.entries.forEach { c ->
                        TvSettingRow(
                            label = categoryLabel(c),
                            icon = c.icon,
                            danger = c == Category.SignOut,
                            chevron = c == shown,
                            onFocused = { shown = c },
                            onClick = {
                                if (c == Category.SignOut) {
                                    confirmSignOut = true
                                } else {
                                    shown = c
                                    rows["first-${c.name}"]?.let { runCatching { it.requestFocus() } }
                                }
                            },
                            modifier = Modifier
                                .rememberedFocus(memory, "category-${c.name}", initial = c == start)
                                .focusRequester(requester("category-${c.name}"))
                                .testTag("$TV_SETTINGS_CATEGORY.${c.name}"),
                        )
                    }
                }
                // The category's rows: x = 376 to 912.
                LazyColumn(
                    Modifier
                        .padding(start = PANE_START - PleyTv.Gutter - CATEGORIES_WIDTH, end = TvSafeArea.Horizontal)
                        .fillMaxSize()
                        .heldKeyStaysIn(HeldEnds.UpDown)
                        .focusGroup()
                        .testTag(TV_SETTINGS_PANE),
                    contentPadding = PaddingValues(bottom = TvSafeArea.Vertical),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    pane(
                        shown,
                        meState,
                        settings,
                        settingsModel,
                        language,
                        memory = memory,
                        requester = ::requester,
                        pick = pick,
                        onWeb = { web = it },
                        onSignOut = { confirmSignOut = true },
                        onOpen = navigator::open,
                        onRestart = navigator::restart,
                    )
                }
            }
        }
        message?.let { text ->
            Box(Modifier.align(Alignment.TopEnd).padding(top = TvSafeArea.Vertical, end = TvSafeArea.Horizontal)) {
                BasicText(
                    text.asString(),
                    style = PleyType.Tv.Label.copy(color = PleyColors.Fg),
                    modifier = Modifier.background(PleyColors.Surface3, app.pley.android.designsystem.PleyShape.Pill).padding(horizontal = 20.dp, vertical = 10.dp),
                )
            }
        }
        Panels(picking, lastPicking, meState, settings, me, settingsModel, language, languageModel, onClose = { picking = null }, returnTo = lastPicking?.let { requester(rowKeyOf(it)) })
        web?.let { path ->
            val server = meState.profile?.server.orEmpty()
            TvWebDialog(server, path, onDismiss = {
                web = null
                rows["row-web"]?.let { runCatching { it.requestFocus() } }
            })
        }
        if (confirmSignOut) {
            TvDialog(
                title = stringResource(R.string.tv_account_sign_out_title),
                body = stringResource(R.string.tv_account_sign_out_body, meState.profile?.server?.let(::hostPort).orEmpty()),
                onDismiss = { confirmSignOut = false },
            ) {
                val cancel = remember { FocusRequester() }
                LaunchedEffect(Unit) { runCatching { cancel.requestFocus() } }
                TvTextButton(stringResource(R.string.tv_account_cancel), onClick = { confirmSignOut = false }, modifier = Modifier.focusRequester(cancel))
                TvTextButton(stringResource(R.string.tv_account_sign_out), color = PleyColors.DangerText, onClick = {
                    confirmSignOut = false
                    me.signOut()
                })
            }
        }
    }
}

/** The first row of category [c], which Select on the category goes to. */
private fun firstRowOf(c: Category, me: MeUiState): String = when (c) {
    Category.Profile -> if (me.profile?.canSwitch == true) "row-switch" else "row-audio"
    Category.Playback -> "row-quality"
    Category.Audio -> "row-passthrough-${Settings.PASSTHROUGH_FORMATS.first()}"
    Category.Display -> "row-subtitle-size"
    Category.Server -> "row-server"
    Category.Diagnostics -> "row-diagnostics"
    Category.About -> "row-version"
    Category.SignOut -> "row-sign-out"
}

/** The row each panel goes back to. */
private fun rowKeyOf(p: Picking): String = when (p) {
    Picking.Language -> "row-language"
    Picking.AudioLanguage -> "row-audio"
    Picking.SubtitleMode -> "row-subtitles"
    Picking.SubtitleLanguage -> "row-subtitle-language"
    Picking.AlwaysStartAs -> "row-start-as"
    Picking.Quality -> "row-quality"
    Picking.FrameRate -> "row-frame-rate"
    is Picking.PassthroughOf -> "row-passthrough-${p.format}"
    Picking.SubtitleSize -> "row-subtitle-size"
    Picking.SubtitleBackground -> "row-subtitle-background"
    Picking.SubtitleFont -> "row-subtitle-font"
    Picking.SubtitleRaise -> "row-subtitle-position"
}

/** The rows of category [c]; the first one's requester is "first-<category>", for Select on the category. */
private fun LazyListScope.pane(
    c: Category,
    me: MeUiState,
    ui: SettingsUiState,
    model: SettingsViewModel,
    language: LanguageUiState,
    memory: app.pley.android.ui.tv.kit.FocusMemory,
    requester: (String) -> FocusRequester,
    pick: (Picking) -> Unit,
    onWeb: (String) -> Unit,
    onSignOut: () -> Unit,
    onOpen: (PleyKey) -> Unit,
    onRestart: (PleyKey) -> Unit,
) {
    val first = firstRowOf(c, me)
    // The row's own requester (panels give focus back to it), and the category's for its first row (Select on the category).
    fun keyed(key: String): Modifier {
        val m = Modifier.rememberedFocus(memory, key).focusRequester(requester(key))
        return if (key == first) m.focusRequester(requester("first-${c.name}")) else m
    }
    val s = ui.settings
    when (c) {
        Category.Profile -> {
            item(key = "profile-header") { ProfileHeader(me) }
            val profile = me.profile
            if (profile?.canSwitch == true) {
                item(key = "row-switch", contentType = SETTING_ROW) {
                    TvSettingRow(stringResource(R.string.tv_account_switch_profile), onClick = { onRestart(PleyKey.Profiles()) }, icon = PleyIcons.Users, chevron = true, modifier = keyed("row-switch"))
                }
            }
            val defaults = me.trackDefaults
            val editable = defaults?.editable == true
            item(key = "row-audio", contentType = SETTING_ROW) {
                val webOnly = stringResource(R.string.tv_account_change_on_web)
                TvSettingRow(
                    stringResource(R.string.tv_account_audio_language),
                    value = if (defaults == null) null else (TrackLanguages.nameOf(defaults.defaults.audioLanguage, formatWords()) ?: stringResource(R.string.tv_account_file_default)) + if (editable) "" else " · $webOnly",
                    icon = PleyIcons.AudioLines,
                    onClick = { if (editable) pick(Picking.AudioLanguage) else onWeb("/account") },
                    modifier = keyed("row-audio"),
                )
            }
            item(key = "row-subtitles", contentType = SETTING_ROW) {
                val webOnly = stringResource(R.string.tv_account_change_on_web)
                TvSettingRow(
                    stringResource(R.string.tv_account_subtitles),
                    value = defaults?.let { stringResource(modeLabel(it.defaults.subtitleMode)) + if (editable) "" else " · $webOnly" },
                    icon = PleyIcons.Captions,
                    onClick = { if (editable) pick(Picking.SubtitleMode) else onWeb("/account") },
                    modifier = keyed("row-subtitles"),
                )
            }
            if (defaults?.readsSubtitles == true) {
                item(key = "row-subtitle-language", contentType = SETTING_ROW) {
                    val webOnly = stringResource(R.string.tv_account_change_on_web)
                    TvSettingRow(
                        stringResource(R.string.tv_account_subtitle_language),
                        value = (TrackLanguages.nameOf(defaults.defaults.subtitleLanguage, formatWords()) ?: stringResource(R.string.tv_account_same_as_audio)) + if (editable) "" else " · $webOnly",
                        icon = PleyIcons.Captions,
                        onClick = { if (editable) pick(Picking.SubtitleLanguage) else onWeb("/account") },
                        modifier = keyed("row-subtitle-language"),
                    )
                }
            }
            // The account's: on every device it signs in on.
            if (language.available) {
                item(key = "row-language", contentType = SETTING_ROW) {
                    TvSettingRow(
                        stringResource(ScreensR.string.language_title),
                        value = language.value.asString(),
                        icon = PleyIcons.Globe,
                        onClick = { pick(Picking.Language) },
                        modifier = keyed("row-language"),
                    )
                }
            }
            item(key = "row-history", contentType = SETTING_ROW) {
                TvSettingRow(stringResource(R.string.tv_account_history), onClick = { onOpen(PleyKey.History) }, icon = PleyIcons.History, chevron = true, modifier = keyed("row-history"))
            }
            // The Libraries page sorts them; with three or fewer, the rail lists them and has no way there.
            item(key = "row-sort-libraries", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_account_sort_libraries),
                    value = stringResource(R.string.tv_account_sort_libraries_value),
                    onClick = { onOpen(PleyKey.Libraries) },
                    icon = PleyIcons.ArrowUpDown,
                    chevron = true,
                    modifier = keyed("row-sort-libraries"),
                )
            }
            if (ui.profiles.size > 1) {
                item(key = "row-start-as", contentType = SETTING_ROW) {
                    val chosen = ui.profiles.firstOrNull { it.id == s.alwaysStartAs }?.name
                    TvSettingRow(
                        stringResource(R.string.tv_account_always_start_as),
                        value = chosen ?: stringResource(R.string.tv_account_ask_who),
                        icon = PleyIcons.UserRound,
                        onClick = { pick(Picking.AlwaysStartAs) },
                        modifier = keyed("row-start-as"),
                    )
                }
            }
            item(key = "row-web", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_account_manage_web),
                    value = stringResource(R.string.tv_account_manage_web_value),
                    icon = PleyIcons.AppWindow,
                    onClick = { onWeb("/account") },
                    modifier = keyed("row-web"),
                )
            }
        }
        Category.Playback -> {
            item(key = "row-quality", contentType = SETTING_ROW) {
                TvSettingRow(stringResource(R.string.tv_settings_quality), value = qualityLabel(s.qualityOnHomeNetwork), icon = PleyIcons.Gauge, onClick = { pick(Picking.Quality) }, modifier = keyed("row-quality"))
            }
            item(key = "row-frame-rate", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_refresh_rate),
                    value = frameRateLabel(ui.frameRate) + " · " + frameRateHint(ui.frameRate),
                    icon = PleyIcons.Monitor,
                    onClick = { pick(Picking.FrameRate) },
                    modifier = keyed("row-frame-rate"),
                )
            }
            item(key = "row-autoplay", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_autoplay),
                    value = stringResource(R.string.tv_settings_autoplay_hint),
                    checked = s.autoplayNext,
                    icon = PleyIcons.SkipForward,
                    onClick = { model.setAutoplayNext(!s.autoplayNext) },
                    modifier = keyed("row-autoplay"),
                )
            }
            item(key = "row-details", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_playback_details),
                    value = stringResource(R.string.tv_settings_playback_details_hint),
                    checked = s.showPlaybackDetails,
                    icon = PleyIcons.Info,
                    onClick = { model.setPlaybackDetails(!s.showPlaybackDetails) },
                    modifier = keyed("row-details"),
                )
            }
            item(key = "advanced", contentType = SETTING_HEADING) { TvPaneHeading(stringResource(R.string.tv_settings_advanced)) }
            item(key = "row-tunneling", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_tunneling),
                    value = stringResource(R.string.tv_settings_tunneling_hint),
                    checked = s.tunneling,
                    icon = PleyIcons.Cpu,
                    onClick = { model.setTunneling(!s.tunneling) },
                    modifier = keyed("row-tunneling"),
                )
            }
            if (ui.debuggable) {
                item(key = "row-software", contentType = SETTING_ROW) {
                    TvSettingRow(
                        stringResource(R.string.tv_settings_software_video),
                        value = stringResource(R.string.tv_settings_software_video_hint),
                        checked = s.allowSoftwareVideo,
                        icon = PleyIcons.Cpu,
                        onClick = { model.setSoftwareVideo(!s.allowSoftwareVideo) },
                        modifier = keyed("row-software"),
                    )
                }
            }
        }
        Category.Audio -> {
            item(key = "audio-intro") {
                BasicText(stringResource(R.string.tv_settings_passthrough_intro), style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = Modifier.padding(start = 20.dp, bottom = 8.dp))
            }
            Settings.PASSTHROUGH_FORMATS.forEach { format ->
                item(key = "row-passthrough-$format", contentType = SETTING_ROW) {
                    TvSettingRow(
                        formatLabel(format),
                        value = passthroughLabel(s.passthrough(format)),
                        icon = PleyIcons.Volume2,
                        onClick = { pick(Picking.PassthroughOf(format)) },
                        modifier = keyed("row-passthrough-$format"),
                    )
                }
            }
        }
        Category.Display -> {
            item(key = "row-subtitle-size", contentType = SETTING_ROW) {
                TvSettingRow(stringResource(R.string.tv_settings_subtitle_size), value = stringResource(sizeLabel(s.subtitleSize)), icon = PleyIcons.Captions, onClick = { pick(Picking.SubtitleSize) }, modifier = keyed("row-subtitle-size"))
            }
            item(key = "row-subtitle-background", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_subtitle_background),
                    value = stringResource(backgroundLabel(s.subtitleBackground)),
                    icon = PleyIcons.Square,
                    onClick = { pick(Picking.SubtitleBackground) },
                    modifier = keyed("row-subtitle-background"),
                )
            }
            item(key = "row-subtitle-font", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_subtitle_font),
                    value = stringResource(fontLabel(s.subtitleFont)),
                    icon = PleyIcons.Pencil,
                    onClick = { pick(Picking.SubtitleFont) },
                    modifier = keyed("row-subtitle-font"),
                )
            }
            item(key = "row-subtitle-position", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_subtitle_position),
                    value = stringResource(raiseLabel(s.subtitleRaise)),
                    icon = PleyIcons.ArrowUp,
                    onClick = { pick(Picking.SubtitleRaise) },
                    modifier = keyed("row-subtitle-position"),
                )
            }
            item(key = "row-reduce-motion", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_reduce_motion),
                    value = stringResource(R.string.tv_settings_reduce_motion_hint),
                    checked = s.reduceMotion,
                    icon = PleyIcons.Sparkles,
                    onClick = { model.setReduceMotion(!s.reduceMotion) },
                    modifier = keyed("row-reduce-motion"),
                )
            }
        }
        Category.Server -> {
            item(key = "row-server", contentType = SETTING_ROW) {
                TvSettingRow(
                    me.profile?.server?.let(::hostPort) ?: stringResource(R.string.tv_settings_server_unknown),
                    value = stringResource(R.string.tv_settings_server_value),
                    icon = PleyIcons.Server,
                    onClick = {},
                    modifier = keyed("row-server"),
                )
            }
            item(key = "row-switch-server", contentType = SETTING_ROW) {
                TvSettingRow(stringResource(R.string.tv_settings_switch_server), onClick = { onRestart(PleyKey.Servers) }, icon = PleyIcons.ArrowLeft, chevron = true, modifier = keyed("row-switch-server"))
            }
        }
        Category.Diagnostics -> {
            item(key = "row-diagnostics", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_settings_open_diagnostics),
                    value = stringResource(R.string.tv_settings_open_diagnostics_value),
                    icon = PleyIcons.Activity,
                    chevron = true,
                    onClick = { onOpen(PleyKey.Diagnostics) },
                    modifier = keyed("row-diagnostics"),
                )
            }
            item(key = "row-frame-counter", contentType = SETTING_ROW) {
                val context = LocalContext.current
                val shown by FrameRate.on.collectAsStateWithLifecycle()
                TvSettingRow(
                    stringResource(R.string.tv_settings_show_frame_rate),
                    value = stringResource(R.string.tv_settings_show_frame_rate_hint),
                    checked = shown,
                    icon = PleyIcons.Gauge,
                    onClick = { FrameRate.show(context, !shown) },
                    modifier = keyed("row-frame-counter"),
                )
            }
            item(key = "facts") { DeviceFacts() }
        }
        Category.About -> {
            item(key = "row-version", contentType = SETTING_ROW) {
                val context = LocalContext.current
                TvSettingRow(stringResource(R.string.tv_settings_version), value = versionOf(context), icon = PleyIcons.Info, onClick = {}, modifier = keyed("row-version"))
            }
            item(key = "licences", contentType = SETTING_HEADING) { TvPaneHeading(stringResource(R.string.tv_settings_licences)) }
            licences().forEachIndexed { i, (name, licence) ->
                item(key = "licence-$i", contentType = SETTING_ROW) {
                    TvSettingRow(name, value = licence, onClick = {}, modifier = keyed("licence-$i"))
                }
            }
            // Discover's titles and pictures: TMDB's terms ask every app that shows them to say this.
            item(key = "credits", contentType = SETTING_HEADING) { TvPaneHeading(stringResource(R.string.tv_settings_credits)) }
            item(key = "row-tmdb", contentType = SETTING_ROW) {
                TvSettingRow("TMDB", value = stringResource(ScreensR.string.requests_credit_about), onClick = {}, modifier = keyed("row-tmdb"))
            }
        }
        Category.SignOut -> {
            item(key = "sign-out-intro") {
                BasicText(
                    stringResource(R.string.tv_account_sign_out_intro, me.profile?.server?.let(::hostPort).orEmpty()),
                    style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                    modifier = Modifier.padding(start = 20.dp, bottom = 8.dp),
                )
            }
            item(key = "row-sign-out", contentType = SETTING_ROW) {
                TvSettingRow(
                    stringResource(R.string.tv_account_sign_out),
                    danger = true,
                    icon = PleyIcons.LogOut,
                    enabled = !me.signingOut,
                    onClick = onSignOut,
                    modifier = keyed("row-sign-out"),
                )
            }
        }
    }
}

/** Who is signed in: the avatar, the name, what kind of profile and the server. */
@Composable
private fun ProfileHeader(me: MeUiState) {
    val profile = me.profile ?: return Spacer(Modifier.height(60.dp))
    Row(Modifier.fillMaxWidth().padding(start = 20.dp, bottom = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        ProfileAvatar(ProfileTile(profile.id, profile.name, profile.avatarUrl, profile.avatarColor, Needs.NONE, current = true), 48.dp)
        Spacer(Modifier.width(16.dp))
        Column {
            BasicText(profile.name, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicText(
                // A managed profile says so; an account needs no word for what it is.
                listOfNotNull(
                    if (profile.managed) stringResource(R.string.tv_account_role_managed) else null,
                    hostPort(profile.server),
                ).joinToString(" · "),
                style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
            )
        }
    }
}

/** The device's facts under "Open diagnostics". */
@Composable
private fun DeviceFacts() {
    val app = LocalContext.current.applicationContext
    val facts = remember(app) { (app as? PleyGraphOwner)?.screensGraph?.device?.let { DiagnosticsViewModel.deviceFacts(it) }.orEmpty() }
    Column(Modifier.padding(start = 20.dp, top = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        facts.forEach { fact ->
            Row {
                BasicText(fact.label, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), modifier = Modifier.width(150.dp))
                BasicText(fact.value, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg))
            }
        }
    }
}

/** The side panels of the choices; [shown] stays drawn while the panel slides away. */
@Composable
private fun Panels(
    open: Picking?,
    shown: Picking?,
    me: MeUiState,
    ui: SettingsUiState,
    model: MeViewModel,
    settings: SettingsViewModel,
    language: LanguageUiState,
    languageModel: LanguageViewModel,
    onClose: () -> Unit,
    returnTo: FocusRequester?,
) {
    val s = ui.settings
    val defaults = me.trackDefaults?.defaults
    val visible = open != null
    val words = formatWords()
    val languages = remember(words) { TrackLanguages.all(words) }
    when (shown) {
        Picking.Language -> TvChoicePanel(
            visible,
            stringResource(ScreensR.string.language_title),
            language.choices.map { TvChoice(it, language.label(it).asString()) },
            language.chosen,
            languageModel::choose,
            onClose,
            returnTo,
        )
        Picking.AudioLanguage -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_account_audio_language),
            listOf(TvChoice<String?>(null, stringResource(R.string.tv_account_file_default))) + languages.map { TvChoice<String?>(it.first, it.second) },
            defaults?.audioLanguage,
            model::setAudioLanguage,
            onClose,
            returnTo,
        )
        Picking.SubtitleMode -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_account_subtitles),
            SUBTITLE_MODES.map { TvChoice(it, stringResource(modeLabel(it)), stringResource(modeHint(it))) },
            defaults?.subtitleMode ?: SubtitleMode.Forced,
            model::setSubtitleMode,
            onClose,
            returnTo,
        )
        Picking.SubtitleLanguage -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_account_subtitle_language),
            listOf(TvChoice<String?>(null, stringResource(R.string.tv_account_same_as_audio))) + languages.map { TvChoice<String?>(it.first, it.second) },
            defaults?.subtitleLanguage,
            model::setSubtitleLanguage,
            onClose,
            returnTo,
        )
        Picking.AlwaysStartAs -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_account_always_start_as),
            listOf(TvChoice<Long?>(null, stringResource(R.string.tv_account_ask_who))) + ui.profiles.map { TvChoice<Long?>(it.id, it.name) },
            s.alwaysStartAs,
            settings::setAlwaysStartAs,
            onClose,
            returnTo,
        )
        Picking.Quality -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_settings_quality),
            SettingsViewModel.QUALITIES.map { TvChoice(it.id, qualityLabel(it.id), qualityHint(it.id)) },
            SettingsViewModel.presetOf(s.qualityOnHomeNetwork).id,
            { id -> settings.setQuality(mobile = false, SettingsViewModel.presetOf(id)) },
            onClose,
            returnTo,
        )
        Picking.FrameRate -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_settings_refresh_rate),
            ui.frameRateChoices.map { TvChoice(it, frameRateLabel(it), frameRateHint(it)) },
            ui.frameRate,
            settings::setFrameRate,
            onClose,
            returnTo,
        )
        is Picking.PassthroughOf -> TvChoicePanel(
            visible,
            formatLabel(shown.format),
            Passthrough.entries.map { TvChoice(it, passthroughLabel(it), passthroughHint(it)) },
            s.passthrough(shown.format),
            { settings.setPassthrough(shown.format, it) },
            onClose,
            returnTo,
        )
        Picking.SubtitleSize -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_settings_subtitle_size),
            Settings.SUBTITLE_SIZES.map { TvChoice(it, stringResource(sizeLabel(it))) },
            s.subtitleSize,
            settings::setSubtitleSize,
            onClose,
            returnTo,
        )
        Picking.SubtitleBackground -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_settings_subtitle_background),
            SubtitleBackground.entries.map { TvChoice(it, stringResource(backgroundLabel(it))) },
            s.subtitleBackground,
            settings::setSubtitleBackground,
            onClose,
            returnTo,
        )
        Picking.SubtitleFont -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_settings_subtitle_font),
            CaptionLook.fonts().map { TvChoice(it, stringResource(fontLabel(it))) },
            s.subtitleFont,
            settings::setSubtitleFont,
            onClose,
            returnTo,
        )
        Picking.SubtitleRaise -> TvChoicePanel(
            visible,
            stringResource(R.string.tv_settings_subtitle_position),
            Settings.SUBTITLE_RAISES.map { TvChoice(it, stringResource(raiseLabel(it))) },
            s.subtitleRaise,
            settings::setSubtitleRaise,
            onClose,
            returnTo,
        )
        null -> Unit
    }
}

@Composable
private fun categoryLabel(c: Category): String = stringResource(
    when (c) {
        Category.Profile -> R.string.tv_settings_profile
        Category.Playback -> R.string.tv_settings_playback
        Category.Audio -> R.string.tv_settings_audio
        Category.Display -> R.string.tv_settings_display
        Category.Server -> R.string.tv_settings_server
        Category.Diagnostics -> R.string.tv_settings_diagnostics
        Category.About -> R.string.tv_settings_about
        Category.SignOut -> R.string.tv_account_sign_out
    },
)

internal fun modeLabel(mode: SubtitleMode): Int = when (mode) {
    SubtitleMode.Off -> R.string.tv_account_subtitles_off
    SubtitleMode.Foreign -> R.string.tv_account_subtitles_foreign
    SubtitleMode.Always -> R.string.tv_account_subtitles_always
    else -> R.string.tv_account_subtitles_forced
}

internal fun modeHint(mode: SubtitleMode): Int = when (mode) {
    SubtitleMode.Off -> R.string.tv_account_subtitles_off_hint
    SubtitleMode.Foreign -> R.string.tv_account_subtitles_foreign_hint
    SubtitleMode.Always -> R.string.tv_account_subtitles_always_hint
    else -> R.string.tv_account_subtitles_forced_hint
}

/** What Original and Automatic do, under their names; `null` for the transcodes, whose names say it. */
@Composable
private fun qualityHint(value: String): String? {
    val preset = SettingsViewModel.presetOf(value)
    return when {
        preset.original -> stringResource(R.string.tv_settings_quality_original_hint)
        preset.automatic -> stringResource(R.string.tv_settings_quality_automatic_hint)
        else -> null
    }
}

/** "Original", "Automatic", "8 Mbps 1080p", as the player's menu says them. */
@Composable
private fun qualityLabel(value: String): String {
    val preset = SettingsViewModel.presetOf(value)
    if (preset.original) return stringResource(R.string.tv_settings_quality_original)
    if (preset.automatic) return stringResource(R.string.tv_settings_quality_automatic)
    val lines = when (preset.maxHeight) {
        2160 -> "4K"
        null -> ""
        else -> "${preset.maxHeight}p"
    }
    return stringResource(R.string.tv_settings_quality_preset, lines, (preset.maxBitrateKbps ?: 0) / 1000)
}

@Composable
private fun frameRateLabel(choice: FrameRateMatching): String = stringResource(
    when (choice) {
        FrameRateMatching.Off -> R.string.tv_settings_refresh_off
        FrameRateMatching.Always -> R.string.tv_settings_refresh_always
        else -> R.string.tv_settings_refresh_auto
    },
)

@Composable
private fun frameRateHint(choice: FrameRateMatching): String = stringResource(
    when (choice) {
        FrameRateMatching.Off -> R.string.tv_settings_refresh_off_hint
        FrameRateMatching.Always -> R.string.tv_settings_refresh_always_hint
        else -> R.string.tv_settings_refresh_auto_hint
    },
)

@Composable
private fun formatLabel(format: String): String = stringResource(
    when (format) {
        "ac3" -> R.string.tv_settings_format_ac3
        "eac3" -> R.string.tv_settings_format_eac3
        "eac3-joc" -> R.string.tv_settings_format_atmos
        "truehd" -> R.string.tv_settings_format_truehd
        "dts" -> R.string.tv_settings_format_dts
        else -> R.string.tv_settings_format_dts_hd
    },
)

@Composable
private fun passthroughLabel(choice: Passthrough): String = stringResource(
    when (choice) {
        Passthrough.Auto -> R.string.tv_settings_passthrough_auto
        Passthrough.On -> R.string.tv_settings_passthrough_on
        Passthrough.Off -> R.string.tv_settings_passthrough_off
    },
)

@Composable
private fun passthroughHint(choice: Passthrough): String = stringResource(
    when (choice) {
        Passthrough.Auto -> R.string.tv_settings_passthrough_auto_hint
        Passthrough.On -> R.string.tv_settings_passthrough_on_hint
        Passthrough.Off -> R.string.tv_settings_passthrough_off_hint
    },
)

private fun sizeLabel(size: Float): Int = when (Settings.SUBTITLE_SIZES.indexOf(size)) {
    0 -> R.string.tv_settings_size_small
    2 -> R.string.tv_settings_size_large
    3 -> R.string.tv_settings_size_largest
    else -> R.string.tv_settings_size_medium
}

private fun backgroundLabel(background: SubtitleBackground): Int = when (background) {
    SubtitleBackground.None -> R.string.tv_settings_background_none
    SubtitleBackground.DarkBox -> R.string.tv_settings_background_box
}

internal fun fontLabel(font: SubtitleFont): Int = when (font) {
    SubtitleFont.Sans -> R.string.tv_settings_font_sans
    SubtitleFont.Serif -> R.string.tv_settings_font_serif
    SubtitleFont.Monospace -> R.string.tv_settings_font_monospace
    SubtitleFont.Casual -> R.string.tv_settings_font_casual
}

internal fun raiseLabel(raise: Float): Int = when (Settings.SUBTITLE_RAISES.indexOf(raise)) {
    1 -> R.string.tv_settings_position_raised
    2 -> R.string.tv_settings_position_higher
    3 -> R.string.tv_settings_position_highest
    else -> R.string.tv_settings_position_bottom
}

/** The app's version and commit, or an empty string when the system won't say. */
internal fun versionOf(context: android.content.Context): String = app.pley.android.common.AppBuild.label(context)

/** The libraries in the app, with their licences. */
@OptIn(UnstableApi::class)
internal fun licences(): List<Pair<String, String>> = listOf(
    "AndroidX (Compose, TV Material, Navigation…)" to "Apache-2.0",
    "Media3 ${MediaLibraryInfo.VERSION}" to "Apache-2.0",
    "OkHttp ${okhttp3.OkHttp.VERSION}" to "Apache-2.0",
    "Coil" to "Apache-2.0",
    "kotlinx.coroutines, kotlinx.serialization" to "Apache-2.0",
    "Lucide icons" to "ISC",
    "QR Code generator (Nayuki)" to "MIT",
)

private val CATEGORIES_WIDTH = 256.dp

/** Where the rows start: x = 376. */
private val PANE_START = 376.dp

private const val MESSAGE_MS = 3_500L

/** The content types of a pane's list, so a row scrolled in reuses one scrolled out. */
private const val SETTING_ROW = "row"
private const val SETTING_HEADING = "heading"

internal const val TV_SETTINGS_PAGE = "pley.tv.settings"
internal const val TV_SETTINGS_PANE = "pley.tv.settings.pane"
internal const val TV_SETTINGS_CATEGORY = "pley.tv.settings.category"
