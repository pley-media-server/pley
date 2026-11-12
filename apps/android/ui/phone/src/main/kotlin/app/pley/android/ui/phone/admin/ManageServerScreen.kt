package app.pley.android.ui.phone.admin

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.AdminAccess
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminSection
import app.pley.android.screens.admin.ManageServerViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.RowDivider
import app.pley.android.ui.phone.account.SettingsGroup
import app.pley.android.ui.phone.account.SettingsRow
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell

/**
 * Manage server (docs/ANDROID.md, "Managing the server"): the parts of
 * Settings in the web app a phone has, in its order, each opening its page,
 * and a line saying where the others are. An
 * administrator signed in before the app could manage the server is asked
 * to sign in again; anyone else is told only administrators can.
 */
@Composable
internal fun ManageServerScreen() {
    val model: ManageServerViewModel = viewModel(factory = ManageServerViewModel.factory)
    val access by model.access.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(MANAGE_SERVER_PAGE)) {
        PleyTopAppBar(title = stringResource(R.string.admin_title), onBack = navigator::back, scrolled = { list.canScrollBackward })
        when (access) {
            AdminAccess.Manages -> AdminColumn(list) {
                for ((n, group) in GROUPS.withIndex()) {
                    item(key = n) {
                        val footer = if (n == GROUPS.lastIndex) stringResource(R.string.admin_web_only) else null
                        SettingsGroup(null, Modifier.adminWidth, footer = footer) {
                            for ((i, section) in group.withIndex()) {
                                if (i > 0) RowDivider()
                                SettingsRow(
                                    stringResource(titleOf(section)),
                                    value = stringResource(valueOf(section)),
                                    icon = iconOf(section),
                                    onClick = { navigator.open(PleyKey.Manage(section)) },
                                    modifier = Modifier.testTag(sectionTag(section)),
                                )
                            }
                        }
                    }
                }
            }
            AdminAccess.SignInAgain -> EmptyState(
                title = stringResource(R.string.admin_sign_in_title),
                body = stringResource(R.string.admin_sign_in_body),
                icon = PleyIcons.LogOut,
                action = { PrimaryButton(stringResource(R.string.admin_sign_in_action), onClick = shell::signInAgain) },
            )
            AdminAccess.None -> EmptyState(
                title = stringResource(R.string.admin_not_admin_title),
                body = stringResource(R.string.admin_not_admin_body),
                icon = PleyIcons.Lock,
            )
            null -> Unit
        }
    }
}

/** The parts, grouped as people look for them: what goes on, what is shared, and with whom. */
private val GROUPS = listOf(
    listOf(AdminSection.Dashboard),
    listOf(AdminSection.Libraries),
    listOf(AdminSection.Users, AdminSection.Invitations),
)

/** A part's name, as the web's Settings names it. */
internal fun titleOf(section: AdminSection): Int = when (section) {
    AdminSection.Dashboard -> R.string.admin_section_dashboard
    AdminSection.Libraries -> R.string.admin_section_libraries
    AdminSection.Users -> R.string.admin_section_users
    AdminSection.Invitations -> R.string.admin_section_invitations
}

private fun valueOf(section: AdminSection): Int = when (section) {
    AdminSection.Dashboard -> R.string.admin_section_dashboard_value
    AdminSection.Libraries -> R.string.admin_section_libraries_value
    AdminSection.Users -> R.string.admin_section_users_value
    AdminSection.Invitations -> R.string.admin_section_invitations_value
}

/** A part's icon, as the web's Settings shows it. */
internal fun iconOf(section: AdminSection): PleyIcon = when (section) {
    AdminSection.Dashboard -> PleyIcons.LayoutDashboard
    AdminSection.Libraries -> PleyIcons.LibraryBig
    AdminSection.Users -> PleyIcons.Users
    AdminSection.Invitations -> PleyIcons.MailPlus
}

internal fun sectionTag(section: AdminSection): String = "pley.admin.section.${section.name.lowercase()}"

internal const val MANAGE_SERVER_PAGE = "pley.admin.page"
