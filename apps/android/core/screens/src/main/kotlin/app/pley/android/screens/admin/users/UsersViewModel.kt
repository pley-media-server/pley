package app.pley.android.screens.admin.users

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.data.Session
import app.pley.android.model.Library
import app.pley.android.model.ServerInfo
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.UserAccount
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.builtins.ListSerializer

/**
 * Users (the web's Settings → Users): every account and managed profile,
 * with what each may see; one opens its own page. The libraries name a
 * restricted account's access, and the server says whether it requires
 * two-step sign-in (an account that hasn't set it up is marked).
 */
class UsersViewModel(admin: ServerAdmin, private val session: StateFlow<Session?>) : AdminViewModel(admin) {
    val users = resource { get(ListSerializer(UserAccount.serializer()), USERS) }
    val libraries = resource { get(ListSerializer(Library.serializer()), LIBRARIES) }
    val server = resource { get(ServerInfo.serializer(), SERVER) }

    /** What the rows' age limits are shown in ("Up to PG-13"). */
    val ratingSystems = resource { get(RatingSystems.serializer(), RATING_SYSTEMS) }

    /** The signed-in account, marked "(you)". */
    val selfId: Long? get() = session.value?.me?.id

    /** [user]'s picture, on the signed-in server. */
    fun avatar(user: UserAccount): String? = avatarOf(user, session.value?.issuer)

    /** The page shows again after an account's page: what was changed there is read. */
    fun shownAgain() = users.reload()

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val data = screensGraph().data
                UsersViewModel(data.admin, data.session.session)
            }
        }
    }
}

internal val USERS = listOf("api", "users")
internal val LIBRARIES = listOf("api", "libraries")
internal val SERVER = listOf("api", "server")
internal val RATING_SYSTEMS = listOf("api", "ratings", "systems")
internal val LABELS = listOf("api", "labels")
internal val SYSTEM = listOf("api", "system")
