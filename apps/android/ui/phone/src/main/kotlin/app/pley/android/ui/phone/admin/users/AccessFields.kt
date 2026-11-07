package app.pley.android.ui.phone.admin.users

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.screens.admin.users.AGE_LIMITS
import app.pley.android.screens.admin.users.AccessChoice
import app.pley.android.screens.admin.users.MIN_PASSWORD_LENGTH
import app.pley.android.screens.admin.users.PasswordProblem
import app.pley.android.screens.admin.users.toggled
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.Choice
import app.pley.android.ui.phone.account.ChoiceSheet
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.SegmentedControl

/*
 * What an account sees, chosen for an account and for an invitation alike
 * (the web's AccessFields and lib/access.ts): every library or only some,
 * and an age limit. Also the small pieces Users and Invitations share: the
 * tags of a row, a switch row that can be off limits, and the words of a
 * password's problem.
 */

/**
 * The group of an account's (or an invitation's) access: Libraries, which
 * opens [LibrariesSheet], and Age limit, which opens a choice of
 * [AGE_LIMITS], with what the limit hides under it. [libraries]: the
 * server's, `null` while unknown. [pickLibrary]: what the sheet says when
 * "Only some" has none chosen.
 */
@Composable
internal fun AccessGroup(
    access: AccessChoice,
    libraries: List<Library>?,
    onLibraries: (all: Boolean, ids: List<Long>) -> Unit,
    onAgeLimit: (Int?) -> Unit,
    pickLibrary: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    var sheet by remember { mutableStateOf<AccessSheet?>(null) }
    SettingsGroup(null, modifier, footer = stringResource(R.string.admin_users_access_age_limit_hint)) {
        SettingsRow(
            stringResource(R.string.admin_users_access_libraries),
            value = librariesLabel(access.allLibraries, access.libraryIds, libraries),
            icon = PleyIcons.LibraryBig,
            enabled = enabled,
            onClick = { sheet = AccessSheet.Libraries },
        )
        RowDivider()
        SettingsRow(
            stringResource(R.string.admin_users_access_age_limit),
            value = ageLabel(access.maxAge),
            icon = PleyIcons.Lock,
            enabled = enabled,
            onClick = { sheet = AccessSheet.Age },
        )
    }
    when (sheet) {
        AccessSheet.Libraries -> LibrariesSheet(
            all = access.allLibraries,
            ids = access.libraryIds,
            libraries = libraries.orEmpty(),
            pickLibrary = pickLibrary,
            onSave = { all, ids ->
                sheet = null
                onLibraries(all, ids)
            },
            onDismiss = { sheet = null },
        )
        AccessSheet.Age -> {
            val limits = if (access.maxAge in AGE_LIMITS) AGE_LIMITS else AGE_LIMITS + access.maxAge
            ChoiceSheet(
                title = stringResource(R.string.admin_users_access_age_limit),
                choices = limits.map { Choice(it, ageLabel(it)) },
                selected = access.maxAge,
                // The one already set isn't sent again.
                onPick = { if (it != access.maxAge) onAgeLimit(it) },
                onDismiss = { sheet = null },
            )
        }
        null -> Unit
    }
}

private enum class AccessSheet { Libraries, Age }

/**
 * Choosing the libraries: all of them (those added later too), or only
 * the ones checked, saved together. Save with none checked says
 * [pickLibrary] instead, as the web does.
 */
@Composable
internal fun LibrariesSheet(
    all: Boolean,
    ids: List<Long>,
    libraries: List<Library>,
    pickLibrary: String,
    onSave: (all: Boolean, ids: List<Long>) -> Unit,
    onDismiss: () -> Unit,
) {
    var allNow by remember { mutableStateOf(all) }
    var chosen by remember { mutableStateOf(ids) }
    var tried by remember { mutableStateOf(false) }
    val none = !allNow && chosen.isEmpty()
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.admin_users_access_libraries)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            SegmentedControl(
                options = listOf(stringResource(R.string.admin_users_access_all_libraries), stringResource(R.string.admin_users_access_only_some)),
                selected = if (allNow) 0 else 1,
                onSelect = { allNow = it == 0 },
                modifier = Modifier.fillMaxWidth(),
            )
            if (allNow) {
                Text(stringResource(R.string.admin_users_access_includes_later), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            } else if (libraries.isEmpty()) {
                Text(stringResource(R.string.admin_users_access_no_libraries_yet), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            }
        }
        if (!allNow && libraries.isNotEmpty()) {
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 360.dp).padding(top = 8.dp)) {
                items(libraries, key = { it.id }) { library ->
                    val on = library.id in chosen
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 52.dp)
                            .toggleable(value = on, role = Role.Checkbox) { chosen = toggled(chosen, library.id) }
                            .padding(horizontal = 24.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Checkbox(
                            checked = on,
                            onCheckedChange = null,
                            colors = CheckboxDefaults.colors(
                                checkedColor = PleyColors.Accent,
                                uncheckedColor = PleyColors.Control,
                                checkmarkColor = PleyColors.OnAccent,
                            ),
                        )
                        Spacer(Modifier.width(16.dp))
                        Icon(kindIcon(library.kind), contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
                        Spacer(Modifier.width(12.dp))
                        Text(library.name, style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
        Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 16.dp)) {
            if (tried && none) {
                Text(pickLibrary, style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText, modifier = Modifier.padding(bottom = 12.dp))
            }
            PrimaryButton(
                stringResource(R.string.admin_save),
                onClick = {
                    tried = true
                    if (!none) onSave(allNow, if (allNow) ids else chosen)
                },
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** A library's icon, as the web's AccessFields shows it. */
private fun kindIcon(kind: LibraryKind): PleyIcon = when (kind) {
    LibraryKind.Movies -> PleyIcons.Clapperboard
    LibraryKind.Shows -> PleyIcons.Tv
    LibraryKind.Music -> PleyIcons.Music
    LibraryKind.Unknown -> PleyIcons.LibraryBig
}

/** "All libraries", "Movies", "2 libraries", "No libraries". */
@Composable
internal fun librariesLabel(all: Boolean, ids: List<Long>, libraries: List<Library>?): String {
    val only = ids.singleOrNull()?.let { id -> libraries?.firstOrNull { it.id == id } }
    return when {
        all -> stringResource(R.string.admin_users_access_all_libraries)
        ids.isEmpty() -> stringResource(R.string.admin_users_access_no_libraries)
        only != null -> only.name
        else -> pluralStringResource(R.plurals.admin_users_access_libraries_count, ids.size, ids.size)
    }
}

/** An age limit's words: the web's options ("Teens (PG-13)"), else "Up to 12". */
@Composable
internal fun ageLabel(maxAge: Int?): String = when (maxAge) {
    null -> stringResource(R.string.admin_users_age_no_limit)
    0 -> stringResource(R.string.admin_users_age_little_kids)
    7 -> stringResource(R.string.admin_users_age_kids)
    10 -> stringResource(R.string.admin_users_age_older_kids)
    13 -> stringResource(R.string.admin_users_age_teens)
    // The ratings' own names, the same in every language, as on the web.
    14 -> "TV-14"
    15 -> "15+"
    17 -> stringResource(R.string.admin_users_age_adults)
    else -> stringResource(R.string.admin_users_age_up_to, maxAge)
}

/** "Up to 13" for a row's tag; `null` without a limit. */
@Composable
internal fun ageTag(maxAge: Int?): String? = maxAge?.let { stringResource(R.string.admin_users_age_up_to, it) }

/** The words of [problem], for a password's dialog. */
@Composable
internal fun passwordWords(): (PasswordProblem) -> String {
    val resources = LocalResources.current
    return { problem ->
        when (problem) {
            PasswordProblem.TooShort -> resources.getQuantityString(R.plurals.admin_users_password_too_short, MIN_PASSWORD_LENGTH, MIN_PASSWORD_LENGTH)
            PasswordProblem.SameAsUsername -> resources.getString(R.string.admin_users_password_same_as_username)
        }
    }
}

/** How a [Tag] reads: as the web's badges' tones. */
internal enum class TagTone { Neutral, Accent, Info, Success, Danger }

/** A small tag on a row ("Admin", "PIN", "Active"), in [tone]. */
@Composable
internal fun Tag(text: String, tone: TagTone = TagTone.Neutral, icon: PleyIcon? = null) {
    val (fill, ink) = when (tone) {
        TagTone.Neutral -> PleyColors.Surface3 to PleyColors.Muted
        TagTone.Accent -> PleyColors.PrimaryContainer to PleyColors.AccentBright
        TagTone.Info -> PleyColors.Info.copy(alpha = 0.16f) to PleyColors.Info
        TagTone.Success -> PleyColors.Success.copy(alpha = 0.16f) to PleyColors.Success
        TagTone.Danger -> PleyColors.Danger.copy(alpha = 0.16f) to PleyColors.DangerText
    }
    Row(
        Modifier.background(fill, PleyShape.Badge).padding(horizontal = 6.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 12.dp, tint = ink)
            Spacer(Modifier.width(4.dp))
        }
        Text(text, style = MaterialTheme.typography.labelSmall, color = ink, maxLines = 1)
    }
}

/** Text that isn't a row: a group's "Loading…" or "No apps use this account.". */
@Composable
internal fun GroupLine(text: String, color: Color = PleyColors.Muted) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = color, modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 16.dp))
}
