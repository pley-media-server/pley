package app.pley.android.ui.phone.account

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.auth.SignIn
import app.pley.android.common.AppBuild
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.SubtitleMode
import app.pley.android.model.format.formatBytes
import app.pley.android.screens.PleyKey
import app.pley.android.data.AdminAccess
import app.pley.android.screens.account.MeEvent
import app.pley.android.screens.admin.ManageServerViewModel
import app.pley.android.screens.account.MeProfile
import app.pley.android.screens.account.MeViewModel
import app.pley.android.screens.account.SUBTITLE_MODES
import app.pley.android.screens.account.TrackDefaultsUi
import app.pley.android.screens.account.TrackField
import app.pley.android.screens.account.TrackLanguages
import app.pley.android.screens.downloads.DownloadsViewModel
import app.pley.android.screens.requests.RequestsEntryViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.Avatar
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.requests.RequestsRows
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * Me (design spec §4.8; apps/web/src/pages/AccountPage.tsx): who is signed
 * in, with Switch profile; Discover and the person's requests, where the
 * server takes requests; the profile's audio and subtitle defaults,
 * saved on the server and read-only (to change on the web) where the server
 * doesn't let apps change them; watch history; for an administrator,
 * managing the server; this device's downloads, settings and diagnostics;
 * the account on the web, linking a TV, another server; and Sign out,
 * asked first.
 */
@Composable
internal fun MeScreen() {
    val model: MeViewModel = viewModel(factory = MeViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val downloads: DownloadsViewModel = viewModel(factory = DownloadsViewModel.factory)
    val downloaded by downloads.byItem.collectAsStateWithLifecycle()
    val entry: RequestsEntryViewModel = viewModel(factory = RequestsEntryViewModel.factory)
    val requests by entry.entry.collectAsStateWithLifecycle()
    val manage: ManageServerViewModel = viewModel(factory = ManageServerViewModel.factory)
    val adminAccess by manage.access.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val context = LocalContext.current
    val resources = LocalResources.current
    var sheet by rememberSaveable { mutableStateOf<TrackField?>(null) }
    var confirmSignOut by rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                is MeEvent.Saved -> shell.toasts.show(resources.getString(savedMessage(event.field)))
                MeEvent.ReadOnly -> shell.toasts.show(resources.getString(R.string.account_defaults_read_only), error = true)
                is MeEvent.Failed -> shell.report(event.error)
                is MeEvent.SignedOut -> navigator.restart(event.server?.let { PleyKey.SignIn(it) } ?: PleyKey.Servers)
            }
        }
    }

    val openWeb: (String) -> Unit = { path ->
        val server = state.profile?.server ?: shell.server
        if (server != null) {
            try {
                SignIn.open(context, server.trimEnd('/') + path, state.privateTab)
            } catch (e: Exception) {
                shell.report(e)
            }
        }
    }

    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(ME_PAGE)) {
        PleyTopAppBar(title = stringResource(R.string.account_me), scrolled = { list.canScrollBackward })
        val metrics = LocalPhoneMetrics.current
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            item(key = "profile", contentType = "profile") {
                ProfileHeader(state.profile, onSwitch = { navigator.open(PleyKey.Profiles()) })
            }
            if (requests.discover || requests.requests) {
                item(key = "requests", contentType = "group") {
                    Group(null) { RequestsRows(requests) }
                }
            }
            item(key = "defaults", contentType = "group") {
                TrackDefaultsGroup(state.trackDefaults, onOpen = { sheet = it }, onWeb = { openWeb("/account") })
            }
            item(key = "activity", contentType = "group") {
                Group(stringResource(R.string.account_activity)) {
                    SettingsRow(stringResource(R.string.account_history), icon = PleyIcons.History, onClick = { navigator.open(PleyKey.History) })
                }
            }
            if (adminAccess != null && adminAccess != AdminAccess.None) {
                item(key = "server", contentType = "group") {
                    Group(stringResource(R.string.admin_me_group)) {
                        val manages = adminAccess == AdminAccess.Manages
                        SettingsRow(
                            stringResource(R.string.admin_title),
                            value = stringResource(if (manages) R.string.admin_me_value else R.string.admin_me_sign_in),
                            icon = PleyIcons.Server,
                            onClick = { if (manages) navigator.open(PleyKey.ManageServer) else shell.signInAgain() },
                            modifier = Modifier.testTag(ME_MANAGE_SERVER),
                        )
                    }
                }
            }
            item(key = "device", contentType = "group") {
                Group(stringResource(R.string.account_this_device)) {
                    if (downloads.available) {
                        val count = downloaded.size
                        SettingsRow(
                            stringResource(R.string.downloads_title),
                            value = if (count == 0) {
                                stringResource(R.string.downloads_me_value)
                            } else {
                                pluralStringResource(
                                    R.plurals.downloads_me_count,
                                    count,
                                    count,
                                    formatBytes(downloaded.values.sumOf { it.sizeBytes }, formatWords()),
                                )
                            },
                            icon = PleyIcons.Download,
                            onClick = { navigator.open(PleyKey.Downloads) },
                        )
                        RowDivider()
                    }
                    SettingsRow(
                        stringResource(R.string.account_settings),
                        value = stringResource(R.string.account_settings_value),
                        icon = PleyIcons.SlidersHorizontal,
                        onClick = { navigator.open(PleyKey.Settings) },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.account_diagnostics),
                        value = stringResource(R.string.account_diagnostics_value),
                        icon = PleyIcons.Activity,
                        onClick = { navigator.open(PleyKey.Diagnostics) },
                    )
                }
            }
            item(key = "account", contentType = "group") {
                val host = state.profile?.server?.let(::hostPort) ?: shell.server?.let(::hostPort)
                Group(stringResource(R.string.account_account)) {
                    SettingsRow(
                        stringResource(R.string.account_manage_web),
                        value = stringResource(R.string.account_manage_web_value),
                        icon = PleyIcons.AppWindow,
                        onClick = { openWeb("/account") },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.account_link_tv),
                        value = stringResource(R.string.account_link_tv_value),
                        icon = PleyIcons.Tv,
                        onClick = { openWeb("/link") },
                    )
                    RowDivider()
                    SettingsRow(
                        stringResource(R.string.account_switch_server),
                        value = host,
                        icon = PleyIcons.Server,
                        onClick = { navigator.restart(PleyKey.Servers) },
                    )
                }
            }
            item(key = "sign-out", contentType = "group") {
                Group(null) {
                    SettingsRow(
                        stringResource(R.string.account_sign_out),
                        icon = PleyIcons.LogOut,
                        danger = true,
                        enabled = !state.signingOut,
                        onClick = { confirmSignOut = true },
                    )
                }
            }
            item(key = "version", contentType = "footer") {
                Text(
                    stringResource(R.string.account_version, versionOf(context)),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Subtle,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }

    state.trackDefaults?.let { defaults ->
        val words = formatWords()
        val languages = remember(words) { TrackLanguages.all(words) }
        when (sheet) {
            TrackField.AudioLanguage -> ChoiceSheet(
                title = stringResource(R.string.account_audio_language),
                choices = listOf(Choice<String?>(null, stringResource(R.string.account_language_file_default))) + languages.map { Choice<String?>(it.first, it.second) },
                selected = defaults.defaults.audioLanguage,
                onPick = model::setAudioLanguage,
                onDismiss = { sheet = null },
            )
            TrackField.SubtitleMode -> ChoiceSheet(
                title = stringResource(R.string.account_subtitles),
                choices = SUBTITLE_MODES.map { Choice(it, stringResource(modeLabel(it)), stringResource(modeHint(it))) },
                selected = defaults.defaults.subtitleMode,
                onPick = model::setSubtitleMode,
                onDismiss = { sheet = null },
            )
            TrackField.SubtitleLanguage -> ChoiceSheet(
                title = stringResource(R.string.account_subtitle_language),
                choices = listOf(Choice<String?>(null, stringResource(R.string.account_language_same_as_audio))) + languages.map { Choice<String?>(it.first, it.second) },
                selected = defaults.defaults.subtitleLanguage,
                onPick = model::setSubtitleLanguage,
                onDismiss = { sheet = null },
            )
            null -> Unit
        }
    }

    if (confirmSignOut) {
        ConfirmDialog(
            title = stringResource(R.string.account_sign_out_title),
            body = stringResource(R.string.account_sign_out_body, state.profile?.server?.let(::hostPort).orEmpty()),
            confirmLabel = stringResource(R.string.account_sign_out),
            onConfirm = {
                confirmSignOut = false
                model.signOut()
            },
            onDismiss = { confirmSignOut = false },
            destructive = true,
        )
    }
}

/** A group under the page's width cap, so tablets don't stretch rows across. */
@Composable
private fun Group(title: String?, footer: String? = null, content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    SettingsGroup(title, Modifier.widthIn(max = MAX_WIDTH), footer, content)
}

@Composable
private fun ProfileHeader(profile: MeProfile?, onSwitch: () -> Unit) {
    Column(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        if (profile == null) {
            Spacer(Modifier.size(64.dp))
            Spacer(Modifier.height(100.dp))
            return@Column
        }
        Avatar(profile.name, color = profile.avatarColor, imageUrl = profile.avatarUrl, size = 64.dp)
        Spacer(Modifier.height(12.dp))
        Text(
            profile.name,
            style = MaterialTheme.typography.headlineSmall,
            color = PleyColors.Fg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.semantics { heading() },
        )
        Spacer(Modifier.height(4.dp))
        Text(
            // A managed profile says so; an account needs no word for what it is.
            listOfNotNull(
                if (profile.managed) stringResource(R.string.account_role_managed) else null,
                hostPort(profile.server),
            ).joinToString(" · "),
            style = MaterialTheme.typography.bodyMedium,
            color = PleyColors.Muted,
        )
        if (profile.canSwitch) {
            Spacer(Modifier.height(16.dp))
            TonalButton(stringResource(R.string.account_switch_profile), onClick = onSwitch, icon = PleyIcons.Users)
        }
    }
}

@Composable
private fun TrackDefaultsGroup(defaults: TrackDefaultsUi?, onOpen: (TrackField) -> Unit, onWeb: () -> Unit) {
    val editable = defaults?.editable == true && !defaults.saving
    Column(Modifier.widthIn(max = MAX_WIDTH)) {
        Group(
            stringResource(R.string.account_audio_and_subtitles),
            footer = if (defaults?.editable == false) null else stringResource(R.string.account_defaults_footer),
        ) {
            val d = defaults?.defaults
            val words = formatWords()
            SettingsRow(
                stringResource(R.string.account_audio_language),
                value = d?.let { TrackLanguages.nameOf(it.audioLanguage, words) ?: stringResource(R.string.account_language_file_default) },
                icon = PleyIcons.AudioLines,
                enabled = editable,
                onClick = { onOpen(TrackField.AudioLanguage) },
            )
            RowDivider()
            SettingsRow(
                stringResource(R.string.account_subtitles),
                value = d?.let { stringResource(modeLabel(it.subtitleMode)) },
                icon = PleyIcons.Captions,
                enabled = editable,
                onClick = { onOpen(TrackField.SubtitleMode) },
            )
            if (defaults?.readsSubtitles == true) {
                RowDivider()
                SettingsRow(
                    stringResource(R.string.account_subtitle_language),
                    value = TrackLanguages.nameOf(d?.subtitleLanguage, words) ?: stringResource(R.string.account_language_same_as_audio),
                    icon = PleyIcons.Captions,
                    enabled = editable,
                    onClick = { onOpen(TrackField.SubtitleLanguage) },
                )
            }
        }
        if (defaults?.editable == false) {
            Row(Modifier.fillMaxWidth().padding(start = 4.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(R.string.account_defaults_web_only),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Subtle,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                PleyTextButton(stringResource(R.string.account_change_on_web), onClick = onWeb)
            }
        }
    }
}

/** The web's names for the subtitle modes (AccountPage.tsx SUBTITLE_MODES). */
internal fun modeLabel(mode: SubtitleMode): Int = when (mode) {
    SubtitleMode.Off -> R.string.account_subtitles_off
    SubtitleMode.Foreign -> R.string.account_subtitles_foreign
    SubtitleMode.Always -> R.string.account_subtitles_always
    else -> R.string.account_subtitles_forced
}

internal fun modeHint(mode: SubtitleMode): Int = when (mode) {
    SubtitleMode.Off -> R.string.account_subtitles_off_hint
    SubtitleMode.Foreign -> R.string.account_subtitles_foreign_hint
    SubtitleMode.Always -> R.string.account_subtitles_always_hint
    else -> R.string.account_subtitles_forced_hint
}

private fun savedMessage(field: TrackField): Int = when (field) {
    TrackField.AudioLanguage -> R.string.account_saved_audio
    TrackField.SubtitleMode -> R.string.account_saved_subtitles
    TrackField.SubtitleLanguage -> R.string.account_saved_subtitle_language
}

/** The app's version and commit ("0.9.3 (1a2b3c4d5)"), or an empty string when the system won't say. */
internal fun versionOf(context: Context): String = AppBuild.label(context)

/** Rows stop growing at this width (tablets). */
private val MAX_WIDTH = 640.dp

internal const val ME_PAGE = "pley.me.page"
internal const val ME_MANAGE_SERVER = "pley.me.manage-server"
