package app.pley.android.ui.tv.drive

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatClockMs
import app.pley.android.screens.PleyKey
import app.pley.android.screens.drive.DriveLine
import app.pley.android.screens.drive.DrivePage
import app.pley.android.screens.drive.DrivePlace
import app.pley.android.screens.drive.DriveViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator

/**
 * The USB drive's page on TV (the Samsung app's StorageScreen): the
 * drive's make and model ("USB drive" when it doesn't say), and under it
 * where the page is ("/Films/2019"; "/ · 437 GB free of 1.0 TB" at the
 * drive's top), then a line each:
 * ".." first in a folder (Up a folder, which goes Back), its folders, then
 * its videos with their kind, or where they resume with a bar of how far
 * through, and their size. OK on a folder opens it over this page, on a
 * video plays it in Pley's player from where it stopped. The page asks for
 * leave to read the drive as it opens; refused, Open settings leads to the
 * app's page in the TV's settings.
 */
@Composable
internal fun TvDriveScreen(folder: String?) {
    val vm: DriveViewModel = viewModel(factory = DriveViewModel.factory(folder))
    val state by vm.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val context = LocalContext.current
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { vm.permissionsAnswered() }
    LaunchedEffect(state.ask) {
        if (state.ask.isNotEmpty()) {
            vm.asked()
            ask.launch(state.ask.toTypedArray())
        }
    }
    // Shown again, from the player (where each video stopped) or the settings (allowed meanwhile).
    LifecycleResumeEffect(vm) {
        vm.resumed()
        onPauseOrDispose { }
    }
    TvDriveContent(
        page = state.page,
        name = state.name,
        onLine = { line -> if (line.folder) navigator.open(PleyKey.Drive(line.path)) else vm.play(line) },
        onUp = navigator::back,
        onOpenSettings = { openAppSettings(context) },
    )
}

@Composable
internal fun TvDriveContent(page: DrivePage, onLine: (DriveLine) -> Unit, onUp: () -> Unit, onOpenSettings: () -> Unit, name: String? = null) {
    Column(Modifier.fillMaxSize().padding(top = TvSafeArea.Vertical).testTag(TV_DRIVE)) {
        BasicText(
            name ?: stringResource(R.string.tv_drive_title),
            style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal).semantics { heading() },
        )
        Spacer(Modifier.height(8.dp))
        val place = when (page) {
            is DrivePage.Folder -> page.place
            is DrivePage.Unreadable -> page.place
            else -> null
        }
        BasicText(
            place?.let { placeLabel(it) }.orEmpty(),
            style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal).heightIn(min = 18.dp),
        )
        Spacer(Modifier.height(16.dp))
        Box(Modifier.fillMaxSize()) {
            when (page) {
                DrivePage.Loading -> Unit
                DrivePage.Refused -> DriveMessage(
                    PleyIcons.CircleAlert,
                    stringResource(R.string.tv_drive_refused_title),
                    stringResource(R.string.tv_drive_refused_body),
                    up = false,
                    action = stringResource(R.string.tv_drive_open_settings),
                    onAction = onOpenSettings,
                )
                DrivePage.NoDrive -> DriveMessage(
                    PleyIcons.Usb,
                    stringResource(R.string.tv_drive_none_title),
                    stringResource(R.string.tv_drive_none_body),
                    up = false,
                )
                is DrivePage.Unreadable -> {
                    DriveLines(page.up, emptyList(), onLine, onUp)
                    DriveMessage(
                        PleyIcons.CircleAlert,
                        stringResource(R.string.tv_drive_unreadable_title),
                        stringResource(R.string.tv_drive_unreadable_body),
                        up = page.up,
                    )
                }
                is DrivePage.Folder -> {
                    DriveLines(page.up, page.lines, onLine, onUp)
                    if (page.lines.isEmpty()) {
                        DriveMessage(
                            PleyIcons.FolderOpen,
                            stringResource(R.string.tv_drive_empty_title),
                            stringResource(R.string.tv_drive_empty_body),
                            up = page.up,
                        )
                    }
                }
            }
        }
    }
}

/** Where the page is on the drive, as a path: "/Films/2019"; at its top, "/" and how full it is ("/ · 437 GB free of 1.0 TB"). */
@Composable
private fun placeLabel(place: DrivePlace): String = when (place) {
    is DrivePlace.Top -> place.space?.let { "/ · " + stringResource(R.string.tv_drive_space, formatBytes(it.freeBytes), formatBytes(it.totalBytes)) } ?: "/"
    is DrivePlace.Under -> place.folders.joinToString("/", prefix = "/")
}

/**
 * The folder's lines, ".." first under the drive's top. Focus starts on
 * the first folder or video, else on ".."; with no line at all the page's
 * message holds it ([DriveMessage]). Back from a folder opened, it is on
 * that folder's line.
 */
@Composable
private fun DriveLines(up: Boolean, lines: List<DriveLine>, onLine: (DriveLine) -> Unit, onUp: () -> Unit) {
    if (!up && lines.isEmpty()) return
    val rail = LocalTvRailFocus.current
    val memory = rememberFocusMemory()
    val first = lines.firstOrNull()?.path
    LazyColumn(
        Modifier
            .padding(start = PleyTv.Gutter)
            .width(LIST_WIDTH)
            .fillMaxHeight()
            .railOnLeftStopOnRight(rail)
            .heldKeyStaysIn(HeldEnds.UpDown)
            .focusGroup()
            .testTag(TV_DRIVE_LINES),
        contentPadding = PaddingValues(bottom = 120.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        if (up) {
            item(key = UP, contentType = "up") {
                TvDriveLine(
                    icon = PleyIcons.CornerLeftUp,
                    title = UP,
                    subtitle = stringResource(R.string.tv_drive_up),
                    fileSize = "",
                    progress = 0f,
                    onClick = onUp,
                    modifier = Modifier.rememberedFocus(memory, UP, initial = first == null),
                )
            }
        }
        items(lines, key = { it.path }, contentType = { if (it.folder) "folder" else "video" }) { line ->
            TvDriveLine(
                icon = if (line.folder) PleyIcons.Folder else PleyIcons.FilePlay,
                title = line.name,
                subtitle = when {
                    line.folder -> null
                    line.resumeMs > 0 -> stringResource(R.string.tv_drive_resumes, line.kind, formatClockMs(line.resumeMs))
                    else -> line.kind
                },
                fileSize = if (line.folder || line.sizeBytes <= 0) "" else formatBytes(line.sizeBytes),
                progress = line.progress,
                onClick = { onLine(line) },
                modifier = Modifier.rememberedFocus(memory, line.path, initial = line.path == first),
            )
        }
    }
}

/**
 * A line of the drive's page (the Samsung app's track line with an icon):
 * the icon on a tile, the name, what it is or where it resumes, a bar of
 * how far through it is under them, and a video's size at the end. Focused
 * it is an accent pill without scale, as lines in lists are (design spec
 * §5.2 rule 1). The bar is drawn, not laid out.
 */
@Composable
internal fun TvDriveLine(
    icon: PleyIcon,
    title: String,
    subtitle: String?,
    fileSize: String,
    progress: Float,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val secondary = if (focused) PleyColors.OnAccent else PleyColors.Muted
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = LINE_HEIGHT)
            .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Field)
            .clickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier.size(36.dp).background(if (focused) Color.Black.copy(alpha = 0.18f) else PleyColors.Surface2, PleyShape.Field),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, size = 20.dp, tint = secondary)
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            BasicText(title, style = PleyType.Tv.Body.copy(color = if (focused) PleyColors.OnAccent else PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!subtitle.isNullOrEmpty()) {
                BasicText(subtitle, style = PleyType.Tv.Caption.copy(color = secondary), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (progress > 0f) {
                val fill = if (focused) PleyColors.OnAccent else PleyColors.Accent
                Box(
                    Modifier.padding(top = 6.dp).widthIn(max = BAR_WIDTH).fillMaxWidth().height(3.dp).drawBehind {
                        val radius = CornerRadius(size.height / 2)
                        drawRoundRect(PleyColors.ProgressTrackOnArt, cornerRadius = radius)
                        drawRoundRect(fill, size = Size(size.width * progress.coerceAtMost(1f), size.height), cornerRadius = radius)
                    },
                )
            }
        }
        if (fileSize.isNotEmpty()) {
            Spacer(Modifier.width(12.dp))
            BasicText(fileSize, style = PleyType.Tv.Caption.copy(color = secondary))
        }
    }
}

/**
 * What the page says in a folder's place (no drive, refused, nothing to
 * play, can't be read), as the Samsung app has it: centred on the page
 * under a 64 dp icon, under the ".." line when there is one ([up]). Its
 * [action] takes focus; without one, and with no ".." to focus, the
 * message holds focus itself, showing nothing of it, so the rail closes
 * over the page and Left still opens it.
 */
@Composable
private fun DriveMessage(icon: PleyIcon, title: String, body: String, up: Boolean, action: String? = null, onAction: () -> Unit = {}) {
    val rail = LocalTvRailFocus.current
    val holds = action == null && !up
    Box(
        Modifier
            .fillMaxWidth()
            .padding(start = PleyTv.Gutter, top = if (up) LINE_HEIGHT + 24.dp else 40.dp, end = TvSafeArea.Horizontal)
            .railOnLeftStopOnRight(rail)
            .focusGroup(),
    ) {
        Column(
            (if (holds) Modifier.initialFocus().focusable().semantics(mergeDescendants = true) { } else Modifier)
                .fillMaxWidth()
                .testTag(TV_DRIVE_MESSAGE),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(icon, contentDescription = null, size = MESSAGE_ICON, tint = PleyColors.Muted)
            Spacer(Modifier.height(20.dp))
            BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg, textAlign = TextAlign.Center))
            Spacer(Modifier.height(10.dp))
            BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted, textAlign = TextAlign.Center), maxLines = 3)
            if (action != null) {
                Spacer(Modifier.height(24.dp))
                TvPillButton(action, onClick = onAction, modifier = Modifier.initialFocus())
            }
        }
    }
}

/** Opens this app's page in the system's settings (absent on some TVs). */
private fun openAppSettings(context: Context) {
    try {
        context.startActivity(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, "package:${context.packageName}".toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    } catch (_: ActivityNotFoundException) {
        // No settings screen for apps here: nothing to open.
    }
}

/** The line that goes up a folder. */
private const val UP = ".."

/** The lines' width from the gutter (the Samsung app's 720), and their least height with an icon. */
private val LIST_WIDTH = 720.dp
private val LINE_HEIGHT = 56.dp

/** The size of a message's icon. */
private val MESSAGE_ICON = 64.dp

/** How long a line's bar is at most. */
private val BAR_WIDTH = 160.dp

internal const val TV_DRIVE = "tv-drive"
internal const val TV_DRIVE_LINES = "tv-drive-lines"
internal const val TV_DRIVE_MESSAGE = "tv-drive-message"
