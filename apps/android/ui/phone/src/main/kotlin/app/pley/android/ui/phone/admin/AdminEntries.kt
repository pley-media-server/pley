package app.pley.android.ui.phone.admin

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.navigation3.runtime.EntryProviderScope
import androidx.navigation3.runtime.NavKey
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminSection
import app.pley.android.ui.phone.admin.dashboard.DashboardScreen
import app.pley.android.ui.phone.admin.invitations.InvitationsScreen
import app.pley.android.ui.phone.admin.libraries.LibrariesScreen
import app.pley.android.ui.phone.admin.users.UsersScreen
import app.pley.android.ui.phone.shell.Placeholder

/**
 * Managing the server on a phone (Manage server, and each of its parts'
 * pages), added to the app's back stacks; they open above Me.
 */
fun EntryProviderScope<NavKey>.adminEntries() {
    entry<PleyKey.ManageServer> { key -> WithGraph(key) { ManageServerScreen() } }
    entry<PleyKey.Manage> { key -> WithGraph(key) { SectionScreen(key) } }
}

/** The page of [key]'s part: its first page, or the thing [PleyKey.Manage.id] names. */
@Composable
private fun SectionScreen(key: PleyKey.Manage) {
    when (key.section) {
        AdminSection.Dashboard -> DashboardScreen(key.id)
        AdminSection.Libraries -> LibrariesScreen(key.id)
        AdminSection.Users -> UsersScreen(key.id)
        AdminSection.Invitations -> InvitationsScreen(key.id)
    }
}

/**
 * [content], whose ViewModels come from the app's graph; where the
 * Application holds none (the shell's own tests), the shell's placeholder.
 */
@Composable
private fun WithGraph(key: NavKey, content: @Composable () -> Unit) {
    if (LocalContext.current.applicationContext is PleyGraphOwner) content() else Placeholder(key)
}
