package app.pley.android.ui.phone.kit

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SheetState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyElevation
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.ui.phone.R

/**
 * A bottom sheet (design spec §3.4, elevation level 2): [surface2][PleyColors.Surface2]
 * with 20 dp top corners, a 1 dp [surface4][PleyColors.Surface4] hairline
 * along its top, a 32 by 4 handle in [control][PleyColors.Control], over a
 * [bg][PleyColors.Bg] 60 % scrim. [title] heads it in titleMedium. Back,
 * a drag down or a tap on the scrim call [onDismiss].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PleySheet(
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    title: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    PleySheet(onDismiss, rememberModalBottomSheetState(skipPartiallyExpanded = true), modifier, title, content)
}

/** [PleySheet] with its [state], for the shell's sheet scenes, which slide it away before the entry goes. */
@ExperimentalMaterial3Api
@Composable
internal fun PleySheet(
    onDismiss: () -> Unit,
    state: SheetState,
    modifier: Modifier = Modifier,
    title: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        modifier = modifier,
        sheetState = state,
        shape = PleyShape.Sheet,
        containerColor = PleyColors.Surface2,
        contentColor = PleyColors.Fg,
        scrimColor = PleyColors.Scrim,
        tonalElevation = 0.dp,
        dragHandle = { SheetHandle() },
    ) {
        Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 8.dp)) {
            if (title != null) {
                Text(
                    title,
                    style = MaterialTheme.typography.titleMedium,
                    color = PleyColors.Fg,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
                )
            }
            content()
        }
    }
}

@Composable
private fun SheetHandle() {
    Box(
        Modifier
            .fillMaxWidth()
            .height(24.dp)
            .drawBehind {
                val hair = PleyElevation.Level2Hairline.toPx()
                drawLine(PleyColors.Surface4, Offset(PleyShape.SheetRadius.toPx(), hair / 2f), Offset(size.width - PleyShape.SheetRadius.toPx(), hair / 2f), hair)
            },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(32.dp, 4.dp).background(PleyColors.Control, PleyShape.Control))
    }
}

/**
 * One row of a sheet (design spec §3.4): a 56 dp [ListItem] with a 24 dp
 * [icon] and [label]; [danger] rows read in [dangerText][PleyColors.DangerText].
 */
@Composable
fun SheetAction(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: PleyIcon? = null,
    supporting: String? = null,
    danger: Boolean = false,
) {
    val color = if (danger) PleyColors.DangerText else PleyColors.Fg
    ListItem(
        headlineContent = { Text(label, style = MaterialTheme.typography.bodyLarge) },
        supportingContent = supporting?.let { { Text(it, style = MaterialTheme.typography.bodyMedium) } },
        leadingContent = icon?.let { { Icon(it, contentDescription = null, size = 24.dp, tint = color) } },
        colors = ListItemDefaults.colors(
            containerColor = PleyColors.Surface2,
            headlineColor = color,
            supportingColor = PleyColors.Muted,
        ),
        modifier = modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 8.dp),
    )
}

/**
 * Asks before something that can't be undone (design spec §1.5): a
 * [surface2][PleyColors.Surface2] dialog with 16 dp corners, the [title]
 * and [body] (a question alone needs none), Cancel and [confirmLabel]. A
 * [destructive] confirmation reads in [dangerText][PleyColors.DangerText]
 * ("Sign out", "Delete").
 */
@Composable
fun ConfirmDialog(
    title: String,
    body: String?,
    confirmLabel: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    destructive: Boolean = false,
    /** What not doing it is called ("Keep them"); Cancel by default. */
    dismissLabel: String? = null,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = onConfirm,
                colors = ButtonDefaults.textButtonColors(contentColor = if (destructive) PleyColors.DangerText else PleyColors.AccentBright),
            ) { Text(confirmLabel, style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(dismissLabel ?: stringResource(R.string.kit_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(title, style = MaterialTheme.typography.headlineSmall) },
        text = body?.let { { Text(it, style = MaterialTheme.typography.bodyMedium) } },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Muted,
        tonalElevation = 0.dp,
    )
}
