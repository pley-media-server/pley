package app.pley.android.ui.phone.admin.libraries

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.Library
import app.pley.android.screens.admin.libraries.LibraryAdminViewModel
import app.pley.android.screens.admin.libraries.LibraryForm
import app.pley.android.screens.admin.libraries.LibraryPage
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.WhileShown
import app.pley.android.ui.phone.admin.AdminPage
import app.pley.android.ui.phone.admin.TextEntryDialog
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.admin.folders.FolderPickerSheet
import app.pley.android.ui.phone.home.kindIcon
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/** What the library's page has open over it. */
private enum class LibraryDialog { Kind, Name, Path, Delete }

/**
 * A library's page, or a new library's (the web's library form): its kind
 * (a new one's), name and folders, picked on the server or typed, sent
 * together with Add library or Save changes; then, for a library, Scan,
 * Force full rescan and Delete, which asks first. The page closes once the
 * library is made, saved or deleted.
 */
@Composable
internal fun LibraryAdminScreen(id: String) {
    val vm: LibraryAdminViewModel = viewModel(factory = LibraryAdminViewModel.factory(id))
    val load by vm.page.state.collectAsStateWithLifecycle()
    val form by vm.form.collectAsStateWithLifecycle()
    val busy by vm.busy.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val resources = LocalResources.current
    var dialog by rememberSaveable { mutableStateOf<LibraryDialog?>(null) }
    AdminEvents(vm) { librariesSaid(resources, it) }
    LaunchedEffect(vm) { vm.finished.collect { navigator.back() } }
    WhileShown(vm) { vm.watchScans() }

    val library = load.valueOrNull?.library
    val title = when {
        library != null -> stringResource(R.string.admin_libraries_edit_title, library.name)
        vm.isNew -> stringResource(R.string.admin_libraries_add_title)
        else -> stringResource(R.string.admin_libraries_title)
    }
    val current = form
    AdminPage(title = title, load = load, onRetry = vm.page::reload, tag = LIBRARY_PAGE) { page ->
        if (current == null) return@AdminPage
        val defaultName = resources.getString(kindDefaultName(current.kind))
        item(key = "details") {
            SettingsGroup(null, Modifier.adminWidth) {
                if (vm.isNew) {
                    SettingsRow(
                        stringResource(R.string.admin_libraries_type),
                        value = stringResource(kindLabel(current.kind)),
                        icon = kindIcon(current.kind),
                        onClick = { dialog = LibraryDialog.Kind },
                        modifier = Modifier.testTag(LIBRARY_KIND),
                    )
                    RowDivider()
                }
                SettingsRow(
                    stringResource(R.string.admin_libraries_name),
                    value = current.name.ifBlank { defaultName },
                    icon = PleyIcons.Pencil,
                    onClick = { dialog = LibraryDialog.Name },
                    modifier = Modifier.testTag(LIBRARY_NAME),
                )
            }
        }
        item(key = "folders") {
            FolderGroup(
                form = current,
                onBrowse = { vm.folders.open(current.paths.lastOrNull()) },
                onType = { dialog = LibraryDialog.Path },
                onRemove = vm::removePath,
            )
        }
        item(key = "save") {
            PrimaryButton(
                stringResource(if (vm.isNew) R.string.admin_libraries_add_title else R.string.admin_libraries_save_changes),
                onClick = { vm.save(defaultName) },
                enabled = busy == 0 && vm.changed(current, library),
                modifier = Modifier.adminWidth.testTag(LIBRARY_SAVE),
            )
        }
        if (page.library != null) {
            item(key = "scan") { ScanGroup(page, onScan = vm::scan) }
            item(key = "delete") {
                SettingsGroup(null, Modifier.adminWidth) {
                    SettingsRow(
                        stringResource(R.string.admin_libraries_delete_library),
                        icon = PleyIcons.Trash2,
                        danger = true,
                        enabled = busy == 0,
                        onClick = { dialog = LibraryDialog.Delete },
                        modifier = Modifier.testTag(LIBRARY_DELETE),
                    )
                }
            }
        }
    }

    FolderPickerSheet(vm.folders, onSelect = vm::addPath)
    if (current == null) return
    when (dialog) {
        LibraryDialog.Kind -> ChoiceSheet(
            title = stringResource(R.string.admin_libraries_type),
            choices = LIBRARY_KINDS.map { Choice(it, stringResource(kindLabel(it)), stringResource(kindLayout(it))) },
            selected = current.kind,
            onPick = vm::setKind,
            onDismiss = { dialog = null },
        )
        LibraryDialog.Name -> TextEntryDialog(
            title = stringResource(R.string.admin_libraries_name),
            initial = current.name,
            hint = stringResource(kindDefaultName(current.kind)),
            onSave = { name ->
                vm.setName(name)
                dialog = null
            },
            onDismiss = { dialog = null },
        )
        LibraryDialog.Path -> TextEntryDialog(
            title = stringResource(R.string.admin_libraries_folder_path),
            initial = "",
            hint = stringResource(R.string.admin_folders_placeholder),
            confirmLabel = stringResource(R.string.admin_libraries_add_path),
            onSave = { path ->
                vm.addPath(path)
                dialog = null
            },
            onDismiss = { dialog = null },
        )
        LibraryDialog.Delete -> ConfirmDialog(
            title = stringResource(R.string.admin_libraries_delete_title),
            body = stringResource(R.string.admin_libraries_delete_message, library?.name.orEmpty()),
            confirmLabel = stringResource(R.string.admin_libraries_delete),
            destructive = true,
            onConfirm = {
                dialog = null
                vm.delete()
            },
            onDismiss = { dialog = null },
        )
        null -> Unit
    }
}

/**
 * The library's folders, each with a button to take it out; Browse… picks
 * one on the server and the next row types one; the kind's recommended
 * layout under them. Saving without one says so here.
 */
@Composable
private fun FolderGroup(form: LibraryForm, onBrowse: () -> Unit, onType: () -> Unit, onRemove: (String) -> Unit) {
    Column(Modifier.adminWidth) {
        SettingsGroup(
            stringResource(R.string.admin_libraries_folders),
            footer = stringResource(R.string.admin_libraries_layout, stringResource(kindLayout(form.kind))),
        ) {
            for (path in form.paths) {
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(start = 16.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(PleyIcons.Folder, contentDescription = null, size = 24.dp, tint = PleyColors.Muted)
                    Spacer(Modifier.width(16.dp))
                    Text(
                        path,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = PleyColors.Fg,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    PleyIconButton(PleyIcons.X, stringResource(R.string.admin_libraries_remove_path, path), onClick = { onRemove(path) }, tint = PleyColors.Muted)
                }
                RowDivider()
            }
            SettingsRow(
                stringResource(R.string.admin_libraries_browse),
                icon = PleyIcons.FolderOpen,
                onClick = onBrowse,
                modifier = Modifier.testTag(LIBRARY_BROWSE),
            )
            RowDivider()
            SettingsRow(stringResource(R.string.admin_libraries_type_path), icon = PleyIcons.Plus, onClick = onType)
        }
        if (form.noFolder) {
            Text(
                stringResource(R.string.admin_libraries_at_least_one_folder),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.DangerText,
                modifier = Modifier.padding(start = 4.dp, top = 8.dp),
            )
        }
    }
}

/** Scan and Force full rescan, with the library's last scan, or the one under way as it goes. */
@Composable
private fun ScanGroup(page: LibraryPage, onScan: (Boolean) -> Unit) {
    val library: Library = page.library ?: return
    SettingsGroup(null, Modifier.adminWidth) {
        SettingsRow(
            stringResource(R.string.admin_libraries_scan),
            value = if (page.scanning) null else libraryLine(library),
            icon = PleyIcons.ScanSearch,
            enabled = !page.scanning,
            onClick = { onScan(false) },
            modifier = Modifier.testTag(LIBRARY_SCAN_ROW),
        )
        if (page.scanning) ScanProgress(page.scan, Modifier.padding(start = 56.dp, end = 16.dp, bottom = 14.dp))
        RowDivider()
        SettingsRow(
            stringResource(R.string.admin_libraries_force_rescan),
            icon = PleyIcons.RefreshCw,
            enabled = !page.scanning,
            onClick = { onScan(true) },
            modifier = Modifier.testTag(LIBRARY_RESCAN_ROW),
        )
    }
}

internal const val LIBRARY_PAGE = "pley.admin.library"
internal const val LIBRARY_KIND = "pley.admin.library.kind"
internal const val LIBRARY_NAME = "pley.admin.library.name"
internal const val LIBRARY_BROWSE = "pley.admin.library.browse"
internal const val LIBRARY_SAVE = "pley.admin.library.save"
internal const val LIBRARY_SCAN_ROW = "pley.admin.library.scan"
internal const val LIBRARY_RESCAN_ROW = "pley.admin.library.rescan"
internal const val LIBRARY_DELETE = "pley.admin.library.delete"
