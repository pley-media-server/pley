package app.pley.android.screens.admin

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.AdminAccess
import app.pley.android.data.ServerAdmin
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn

/**
 * Whether the profile manages the server from this phone (Me's row, and
 * Manage server's page): an administrator's account allowed `server`; one
 * signed in before the app could ask for it is offered to sign in again.
 */
class ManageServerViewModel(admin: ServerAdmin) : ViewModel() {
    val access: StateFlow<AdminAccess> = admin.access.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AdminAccess.None)

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { ManageServerViewModel(screensGraph().data.admin) }
        }
    }
}
