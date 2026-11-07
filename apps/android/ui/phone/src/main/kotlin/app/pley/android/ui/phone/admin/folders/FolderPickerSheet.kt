package app.pley.android.ui.phone.admin.folders

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.folders.FolderBrowser
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.admin.Entry
import app.pley.android.ui.phone.admin.TextEntryDialog
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton

/**
 * Choosing a folder on the server, as the web's FolderPicker does, in a
 * sheet while [browser] is open: the folder shown (tap it to type a path),
 * up one level, its folders to open, and Select this folder, which hands
 * it to [onSelect] and closes. At the top, the server's drives and places
 * to start from; a folder that can't be read says why, with a way back.
 */
@Composable
internal fun FolderPickerSheet(browser: FolderBrowser, onSelect: (String) -> Unit) {
    val view by browser.view.collectAsStateWithLifecycle()
    val shown = view ?: return
    var typing by rememberSaveable { mutableStateOf(false) }
    PleySheet(onDismiss = browser::close, title = stringResource(R.string.admin_folders_title), modifier = Modifier.testTag(FOLDER_PICKER)) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            PleyIconButton(PleyIcons.CornerLeftUp, stringResource(R.string.admin_folders_up), onClick = browser::up, enabled = !shown.atRoots)
            Spacer(Modifier.width(4.dp))
            val path = (shown.load as? AdminLoad.Ready)?.value?.path ?: shown.path
            Text(
                path ?: stringResource(R.string.admin_folders_placeholder),
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = if (path != null) FontFamily.Monospace else null,
                color = if (path != null) PleyColors.Fg else PleyColors.Subtle,
                // A long path keeps its end, the folder it is.
                maxLines = 1,
                overflow = TextOverflow.StartEllipsis,
                modifier = Modifier
                    .weight(1f)
                    .clickable(role = Role.Button, onClickLabel = stringResource(R.string.admin_folders_path_label)) { typing = true }
                    .padding(vertical = 12.dp, horizontal = 4.dp)
                    .testTag(FOLDER_PATH),
            )
            PleyIconButton(PleyIcons.Pencil, stringResource(R.string.admin_folders_path_label), onClick = { typing = true })
        }
        Box(Modifier.fillMaxWidth().height(LIST_HEIGHT), contentAlignment = Alignment.Center) {
            when (val load = shown.load) {
                AdminLoad.Loading -> DelayedSpinner(stringResource(R.string.admin_loading))
                is AdminLoad.Failed -> Column(
                    Modifier.padding(24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    Text(errorSentence(load.error).asString(), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText, textAlign = TextAlign.Center)
                    TonalButton(stringResource(R.string.admin_folders_back_to_top), onClick = { browser.go(null) })
                }
                is AdminLoad.Ready -> if (load.value.entries.isEmpty()) {
                    Text(stringResource(R.string.admin_folders_empty), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
                } else {
                    LazyColumn(Modifier.fillMaxSize()) {
                        items(load.value.entries, key = { it.path }) { entry ->
                            Row(
                                Modifier
                                    .fillMaxWidth()
                                    .heightIn(min = 52.dp)
                                    .clickable(role = Role.Button) { browser.go(entry.path) }
                                    .padding(horizontal = 24.dp, vertical = 8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Icon(
                                    if (shown.atRoots) PleyIcons.HardDrive else PleyIcons.Folder,
                                    contentDescription = null,
                                    size = 20.dp,
                                    tint = if (shown.atRoots) PleyColors.Subtle else PleyColors.AccentBright,
                                )
                                Spacer(Modifier.width(16.dp))
                                Text(
                                    entry.name,
                                    style = MaterialTheme.typography.bodyLarge,
                                    color = PleyColors.Fg,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f),
                                )
                                Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
                            }
                        }
                    }
                }
            }
        }
        Row(
            Modifier.fillMaxWidth().padding(start = 16.dp, end = 24.dp, top = 8.dp),
            horizontalArrangement = Arrangement.End,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PleyTextButton(stringResource(R.string.admin_cancel), onClick = browser::close)
            Spacer(Modifier.width(8.dp))
            PrimaryButton(
                stringResource(R.string.admin_folders_select),
                onClick = {
                    shown.chosen?.let { chosen ->
                        browser.close()
                        onSelect(chosen)
                    }
                },
                enabled = shown.chosen != null,
                modifier = Modifier.testTag(FOLDER_SELECT),
            )
        }
    }
    if (typing) {
        TextEntryDialog(
            title = stringResource(R.string.admin_folders_path_label),
            initial = shown.chosen ?: shown.path.orEmpty(),
            hint = stringResource(R.string.admin_folders_placeholder),
            confirmLabel = stringResource(R.string.admin_folders_go),
            entry = Entry.Text,
            onSave = { path ->
                typing = false
                browser.go(path)
            },
            onDismiss = { typing = false },
        )
    }
}

/** How tall the list of folders is, so the sheet doesn't jump as folders open. */
private val LIST_HEIGHT = 360.dp

internal const val FOLDER_PICKER = "pley.admin.folders"
internal const val FOLDER_PATH = "pley.admin.folders.path"
internal const val FOLDER_SELECT = "pley.admin.folders.select"
