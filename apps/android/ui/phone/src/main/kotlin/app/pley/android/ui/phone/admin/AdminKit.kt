package app.pley.android.ui.phone.admin

import android.annotation.SuppressLint
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedSecureTextField
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * The pieces of managing the server on a phone (docs/ANDROID.md,
 * "Managing the server"), with the account kit's groups and rows
 * (SettingsGroup, SettingsRow, SwitchRow, ChoiceSheet) and the kit's
 * dialogs and sheets: a page that reads the server ([AdminPage]), what a
 * value is typed into ([TextEntryDialog]), and the toasts of a page's
 * changes ([AdminEvents]).
 */

/** How wide a page of managing the server's column grows, as Settings'. */
internal val ADMIN_MAX_WIDTH: Dp = 640.dp

/**
 * A page of managing the server: the top bar ([title], back, [actions]),
 * then what [load] read: a spinner while it reads, the error with Retry
 * when it couldn't, else [content] in a column as wide as Settings', its
 * groups 28 dp apart. What was shown stays shown when reading it again
 * fails (the toast says why).
 */
@Composable
internal fun <T> AdminPage(
    title: String,
    load: AdminLoad<T>,
    onRetry: () -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
    actions: @Composable RowScope.() -> Unit = {},
    content: LazyListScope.(T) -> Unit,
) {
    val navigator = LocalPhoneNavigator.current
    val list = rememberLazyListState()
    Column(modifier.fillMaxSize().testTag(tag)) {
        PleyTopAppBar(title = title, onBack = navigator::back, scrolled = { list.canScrollBackward }, actions = actions)
        val value = when (load) {
            is AdminLoad.Ready -> load.value
            is AdminLoad.Failed -> load.stale
            AdminLoad.Loading -> null
        }
        when {
            value != null -> AdminColumn(list) { content(value) }
            load is AdminLoad.Failed -> ErrorState(stringResource(R.string.admin_load_failed), load.error, onRetry)
            else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                DelayedSpinner(stringResource(R.string.admin_loading))
            }
        }
    }
}

/** The column of a page's groups, centred and no wider than [ADMIN_MAX_WIDTH]. */
@Composable
internal fun AdminColumn(state: androidx.compose.foundation.lazy.LazyListState = rememberLazyListState(), content: LazyListScope.() -> Unit) {
    val metrics = LocalPhoneMetrics.current
    LazyColumn(
        state = state,
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(28.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        content = content,
    )
}

/**
 * A [Modifier] that keeps a group of a page as wide as Settings' at most.
 * Lint doesn't see a property's getter use its receiver, which this one
 * does.
 */
@get:SuppressLint("ModifierFactoryUnreferencedReceiver")
internal val Modifier.adminWidth: Modifier get() = this.widthIn(max = ADMIN_MAX_WIDTH).fillMaxWidth()

/**
 * Runs [block] while the page is on screen (started), and stops it when it
 * isn't, as the web reads what changes only while it shows: a page above
 * it, the app in the background.
 */
@Composable
internal fun WhileShown(key: Any, block: suspend () -> Unit) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(key, lifecycle) { lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) { block() } }
}

/**
 * The toasts of [model]'s changes: the server's refusals as the shell says
 * them ([app.pley.android.ui.phone.shell.PhoneShellState.report]), and
 * the confirmations [said] has words for.
 */
@Composable
internal fun AdminEvents(model: AdminViewModel, said: (AdminDone) -> String? = { null }) {
    val shell = LocalPhoneShell.current
    LaunchedEffect(model) { model.errors.collect(shell::report) }
    LaunchedEffect(model) {
        model.confirmations.collect { done -> said(done)?.let { shell.toasts.show(it) } }
    }
}

/** How a [TextEntryDialog] takes what is typed. */
internal enum class Entry {
    Text,

    /** Digits only (ports, counts, limits). */
    Number,

    /** Hidden as it is typed, with a button to show it (passwords, keys). */
    Secret,

    /** An address (a URL, a host). */
    Address,
}

/**
 * A value typed into a dialog: [title], the field starting on [initial]
 * (its [label] above, [hint] under it), Save and Cancel. [check] answers
 * what's wrong with what is typed, which keeps Save from it and shows
 * under the field; `null` when it's fine. Save hands over what was typed,
 * trimmed but for [Entry.Secret].
 */
@Composable
internal fun TextEntryDialog(
    title: String,
    initial: String,
    onSave: (String) -> Unit,
    onDismiss: () -> Unit,
    label: String? = null,
    hint: String? = null,
    entry: Entry = Entry.Text,
    multiLine: Boolean = false,
    confirmLabel: String? = null,
    check: (String) -> String? = { null },
) {
    val state = rememberTextFieldState(initial)
    val focus = remember { FocusRequester() }
    var tried by remember { mutableStateOf(false) }
    val typed = state.text.toString().let { if (entry == Entry.Secret) it else it.trim() }
    val problem = check(typed)
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val colors = OutlinedTextFieldDefaults.colors(
        focusedBorderColor = PleyColors.AccentBright,
        unfocusedBorderColor = PleyColors.Control,
        focusedLabelColor = PleyColors.AccentBright,
        unfocusedLabelColor = PleyColors.Muted,
        cursorColor = PleyColors.AccentBright,
        focusedSupportingTextColor = PleyColors.Muted,
        unfocusedSupportingTextColor = PleyColors.Muted,
        errorSupportingTextColor = PleyColors.DangerText,
    )
    val showProblem = tried && problem != null
    val supporting: (@Composable () -> Unit)? = when {
        showProblem -> ({ Text(problem) })
        hint != null -> ({ Text(hint) })
        else -> null
    }
    val keyboard = KeyboardOptions(
        keyboardType = when (entry) {
            Entry.Number -> KeyboardType.Number
            Entry.Secret -> KeyboardType.Password
            Entry.Address -> KeyboardType.Uri
            Entry.Text -> KeyboardType.Text
        },
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = {
                    tried = true
                    if (problem == null) onSave(typed)
                },
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.AccentBright),
                modifier = Modifier.testTag(TEXT_ENTRY_SAVE),
            ) { Text(confirmLabel ?: stringResource(R.string.admin_save), style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(stringResource(R.string.admin_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(title, style = MaterialTheme.typography.headlineSmall) },
        text = {
            val field = Modifier.fillMaxWidth().focusRequester(focus).testTag(TEXT_ENTRY_FIELD)
            if (entry == Entry.Secret) {
                OutlinedSecureTextField(
                    state = state,
                    modifier = field,
                    label = label?.let { { Text(it) } },
                    supportingText = supporting,
                    isError = showProblem,
                    keyboardOptions = keyboard,
                    shape = PleyShape.Field,
                    colors = colors,
                )
            } else {
                OutlinedTextField(
                    state = state,
                    modifier = field,
                    label = label?.let { { Text(it) } },
                    supportingText = supporting,
                    isError = showProblem,
                    lineLimits = if (multiLine) TextFieldLineLimits.MultiLine(maxHeightInLines = 5) else TextFieldLineLimits.SingleLine,
                    keyboardOptions = keyboard,
                    shape = PleyShape.Field,
                    colors = colors,
                )
            }
        },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Muted,
        tonalElevation = 0.dp,
    )
}

/** What a number typed into a [TextEntryDialog] is, within [range]; `null` when it isn't one. */
internal fun numberIn(text: String, range: LongRange): Long? = text.toLongOrNull()?.takeIf { it in range }

/** The words for a [TextEntryDialog]'s number check: "From 1 to 1000". */
@Composable
internal fun numberProblem(range: LongRange): (String) -> String? {
    val words = LocalResources.current.getString(R.string.admin_number_range, range.first, range.last)
    return { text -> if (numberIn(text, range) == null) words else null }
}

internal const val TEXT_ENTRY_FIELD = "pley.admin.entry.field"
internal const val TEXT_ENTRY_SAVE = "pley.admin.entry.save"
