package app.pley.android.ui.phone.admin.users

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.model.Library
import app.pley.android.model.ServerInfo
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.UserAccount
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminSection
import app.pley.android.screens.admin.users.UsersViewModel
import app.pley.android.screens.admin.users.mustSetUpTwoStep
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.AdminPage
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.Avatar
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * Users (the web's Settings → Users): the accounts and managed profiles,
 * each opening its own page ([id]); [PleyKey.Manage.NEW] adds one.
 */
@Composable
internal fun UsersScreen(id: String?) {
    when (id) {
        null -> UsersList()
        PleyKey.Manage.NEW -> NewUserPage()
        else -> {
            val user = id.toLongOrNull()
            if (user != null) UserAccountPage(user) else UsersList()
        }
    }
}

/** Every account: its picture, name, and tags for its role and access, as the web's list. */
@Composable
private fun UsersList() {
    val model: UsersViewModel = viewModel(factory = UsersViewModel.factory)
    val load by model.users.state.collectAsStateWithLifecycle()
    val libraries by model.libraries.state.collectAsStateWithLifecycle()
    val server by model.server.state.collectAsStateWithLifecycle()
    val ratingSystems by model.ratingSystems.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val add = { navigator.open(PleyKey.Manage(AdminSection.Users, PleyKey.Manage.NEW)) }
    AdminEvents(model)
    ShownAgain(model::shownAgain)
    AdminPage(
        title = stringResource(R.string.admin_section_users),
        load = load,
        onRetry = model.users::reload,
        tag = USERS_PAGE,
        actions = { PleyIconButton(PleyIcons.UserPlus, stringResource(R.string.admin_users_add), onClick = add, modifier = Modifier.testTag(USERS_ADD)) },
    ) { users ->
        item(key = "intro") {
            Text(
                stringResource(R.string.admin_users_description),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.adminWidth.padding(horizontal = 4.dp),
            )
        }
        if (users.isEmpty()) {
            item(key = "empty") {
                EmptyState(
                    title = stringResource(R.string.admin_users_empty),
                    body = null,
                    icon = PleyIcons.UserPlus,
                    action = { PrimaryButton(stringResource(R.string.admin_users_add), onClick = add, icon = PleyIcons.Plus) },
                )
            }
        } else {
            // One group, as the web's list: a household's accounts are a handful, drawn at once.
            item(key = "users") {
                SettingsGroup(null, Modifier.adminWidth) {
                    for ((i, user) in users.withIndex()) {
                        if (i > 0) RowDivider()
                        UserRow(
                            user = user,
                            self = user.id == model.selfId,
                            picture = model.avatar(user),
                            libraries = libraries.valueOrNull,
                            server = server.valueOrNull,
                            systems = ratingSystems.valueOrNull,
                            onOpen = { navigator.open(PleyKey.Manage(AdminSection.Users, user.id.toString())) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun UserRow(
    user: UserAccount,
    self: Boolean,
    picture: String?,
    libraries: List<Library>?,
    server: ServerInfo?,
    systems: RatingSystems?,
    onOpen: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(role = Role.Button, onClick = onOpen)
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .testTag(userTag(user.id)),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Avatar(user.name, color = user.avatarColor, imageUrl = picture, size = 40.dp, modifier = if (user.disabled) Modifier.alpha(0.4f) else Modifier)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    user.name,
                    style = MaterialTheme.typography.bodyLarge,
                    color = if (user.disabled) PleyColors.Muted else PleyColors.Fg,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                val extra = listOfNotNull(
                    "@${user.username}".takeIf { user.name != user.username },
                    if (self) stringResource(R.string.admin_users_you) else null,
                ).joinToString(" ")
                if (extra.isNotEmpty()) {
                    Spacer(Modifier.width(8.dp))
                    Text(extra, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            UserTags(user, libraries, server, systems, Modifier.padding(top = 6.dp))
        }
        Spacer(Modifier.width(12.dp))
        Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
    }
}

/** An account's tags, as the web's badges: role, home, disabled, access, PIN, two-step sign-in. */
@Composable
private fun UserTags(user: UserAccount, libraries: List<Library>?, server: ServerInfo?, systems: RatingSystems?, modifier: Modifier = Modifier) {
    FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (user.isAdmin) Tag(stringResource(R.string.admin_users_badge_admin), TagTone.Accent)
        if (user.managed) Tag(stringResource(R.string.admin_users_badge_managed), TagTone.Info)
        if (user.home) Tag(stringResource(R.string.admin_users_badge_home))
        if (user.disabled) Tag(stringResource(R.string.admin_users_badge_disabled), TagTone.Danger)
        if (!user.isAdmin && !user.allLibraries) Tag(librariesLabel(false, user.libraryIds, libraries))
        if (!user.isAdmin) ageTag(systems, user.maxAgeSystem, user.maxAge, user.birthMonth)?.let { Tag(it) }
        if (user.hasPin) Tag(stringResource(R.string.admin_users_badge_pin), icon = PleyIcons.Lock)
        if (user.twoStep) {
            Tag(stringResource(R.string.admin_users_badge_two_step), TagTone.Success, PleyIcons.ShieldCheck)
        } else if (mustSetUpTwoStep(user, server) && !user.disabled) {
            Tag(stringResource(R.string.admin_users_badge_two_step_missing), TagTone.Danger, PleyIcons.TriangleAlert)
        }
    }
}

/**
 * Calls [onAgain] each time the page shows again after another page above
 * it (an account's, a new one), not the first time: what was changed
 * there is read, as the web's list is after its dialog.
 */
@Composable
internal fun ShownAgain(onAgain: () -> Unit) {
    var seen by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        if (seen) onAgain() else seen = true
    }
}

internal const val USERS_PAGE = "pley.admin.users"
internal const val USERS_ADD = "pley.admin.users.add"

internal fun userTag(id: Long): String = "pley.admin.users.user.$id"
