package app.pley.android.screens.admin.users

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.data.Session
import app.pley.android.model.Library
import app.pley.android.model.RequestAccess
import app.pley.android.model.ServerInfo
import app.pley.android.model.admin.LabelUse
import app.pley.android.model.admin.MetadataKeys
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.UserAccount
import app.pley.android.model.admin.UserApp
import app.pley.android.model.admin.UserDevice
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

/** An account's page: the account, and whether another enabled administrator is left besides it. */
data class UserPage(val user: UserAccount, val otherAdmins: Int) {
    /** Pley always keeps one enabled administrator. */
    val lastAdmin: Boolean get() = user.isAdmin && otherAdmins == 0
}

/**
 * One account's page (the web's user dialog), each change sent as it is
 * made: its name, password, role, whether it is in the home or disabled,
 * requests, what it sees, its PIN; turning its two-step sign-in off,
 * signing it out everywhere or one of its apps, deleting it.
 *
 * The server refuses changes to the administrator's own password, PIN,
 * two-step sign-in, sign-ins, disabling and deleting, and to an app their
 * own name, devices and apps too, which stay in the web app's Account;
 * the page doesn't offer them on one's own account ([self]). From an app
 * it also makes no administrator, nor changes another one's password, PIN
 * or two-step sign-in: the web app does, as either would take whoever
 * holds the phone into it as an administrator.
 */
class UserViewModel(
    admin: ServerAdmin,
    private val session: StateFlow<Session?>,
    val id: Long,
) : AdminViewModel(admin) {
    val page = resource {
        val all = get(ListSerializer(UserAccount.serializer()), USERS)
        val user = all.firstOrNull { it.id == id } ?: throw ApiException(404, "not_found", "user not found")
        UserPage(user, all.count { it.isAdmin && !it.disabled && it.id != id })
    }
    val libraries = resource { get(ListSerializer(Library.serializer()), LIBRARIES) }
    val server = resource { get(ServerInfo.serializer(), SERVER) }
    val ratingSystems = resource { get(RatingSystems.serializer(), RATING_SYSTEMS) }
    val labels = resource { get(ListSerializer(LabelUse.serializer()), LABELS) }
    val metadataKeys = resource { get(MetadataKeys.serializer(), SYSTEM) }
    val devices = resource { if (isSelf()) emptyList() else get(ListSerializer(UserDevice.serializer()), path("devices")) }
    val apps = resource { if (isSelf()) emptyList() else get(ListSerializer(UserApp.serializer()), path("apps")) }

    private val gone = MutableStateFlow<String?>(null)

    /**
     * The name of the account, once deleted: the page says so as it goes
     * back, as a confirmation could arrive after it left.
     */
    val deleted: StateFlow<String?> = gone.asStateFlow()

    /** The account is the signed-in administrator's own, once the session knows who that is. */
    val self: StateFlow<Boolean> =
        session.map { it?.me?.id == id }.stateIn(viewModelScope, SharingStarted.Eagerly, isSelf())

    private fun isSelf(): Boolean = session.value?.me?.id == id

    /** [user]'s picture, on the signed-in server. */
    fun avatar(user: UserAccount): String? = avatarOf(user, session.value?.issuer)

    // The last account read, a reload that failed included: the page still
    // shows it, and acts on it.
    private val user: UserAccount? get() = page.state.value.valueOrNull?.user

    /** The display name; empty: none, the username shows. */
    fun rename(name: String) = patch(
        buildJsonObject { if (name.isEmpty()) put("display_name", JsonNull) else put("display_name", name) },
    )

    /**
     * A new password, which signs the account out everywhere; a managed
     * profile's makes it an account. Its devices and apps are read again,
     * as the server has just signed them out.
     */
    fun setPassword(password: String) = patch(buildJsonObject { put("password", password) }, said = SAVED) {
        devices.reload()
        apps.reload()
    }

    /**
     * No longer an administrator (the web app makes one). The account is
     * given what the page then shows for requests, not what was kept from
     * before it was one, as the web does.
     */
    fun removeAdmin() {
        val user = user ?: return
        patch(
            buildJsonObject {
                put("is_admin", false)
                if (user.requestAccess != null) put("request_access", requestsByDefault(user.managed, user.maxAge).wire)
            },
        )
    }

    fun setHome(on: Boolean) = patch(buildJsonObject { put("home", on) })

    fun setDisabled(on: Boolean) = patch(buildJsonObject { put("disabled", on) })

    fun setRequests(access: RequestAccess) = patch(buildJsonObject { put("request_access", access.wire) })

    /** Every library, or only [ids]: the ids are sent only when they changed. */
    fun setLibraries(all: Boolean, ids: List<Long>) {
        val user = user ?: return
        val body = buildJsonObject {
            if (all != user.allLibraries) put("all_libraries", all)
            if (!all && ids.toSet() != user.libraryIds.toSet()) putJsonArray("library_ids") { ids.forEach { add(it) } }
        }
        if (body.isNotEmpty()) patch(body)
    }

    /**
     * The age limit as [next] has it, what changed of it sent as the web's
     * dialog sends it: a birthday the limit follows, else the limit (`null`:
     * none), which stops one following a birthday; and the rating system
     * it's shown in. An account that asked an administrator for titles
     * stops asking when a limit is added (the server does it).
     */
    fun setAgeLimit(next: AccessChoice) {
        val user = user ?: return
        val body = buildJsonObject {
            val born = next.birthMonth
            if (born != null) {
                if (born != user.birthMonth) put("birth_month", born)
            } else if (next.maxAge != user.maxAge || user.birthMonth != null) {
                put("max_age", next.maxAge?.let { JsonPrimitive(it) } ?: JsonNull)
            }
            // `null`: the metadata region's.
            if (next.maxAgeSystem != user.maxAgeSystem) put("max_age_system", next.maxAgeSystem?.let { JsonPrimitive(it) } ?: JsonNull)
        }
        if (body.isNotEmpty()) patch(body)
    }

    /**
     * The exceptions by label: titles always shown ([show]) and never
     * shown ([hide]); a list is sent only when it changed. The labels are
     * read again after, as one a rule names is offered from then on.
     */
    fun setLabels(show: List<String>, hide: List<String>) {
        val user = user ?: return
        val body = buildJsonObject {
            if (!sameLabels(show, user.showLabels)) putJsonArray("show_labels") { show.forEach { add(it) } }
            if (!sameLabels(hide, user.hideLabels)) putJsonArray("hide_labels") { hide.forEach { add(it) } }
        }
        if (body.isNotEmpty()) patch(body, then = labels::reload)
    }

    fun setPin(pin: String) = patch(buildJsonObject { put("pin", pin) })

    fun removePin() = patch(buildJsonObject { put("pin", JsonNull) })

    /**
     * Turns two-step sign-in off, for someone who lost their phone and
     * recovery codes; while the server requires it, they are signed out
     * everywhere and set it up again at their next sign-in.
     */
    fun turnOffTwoStep() {
        val user = user ?: return
        val required = server.state.value.valueOrNull?.twoStepRequired == true
        change(
            said = AdminDone(if (required) TWO_STEP_SIGNED_OUT else TWO_STEP_OFF, listOf(user.name)),
            after = { page.reload(); devices.reload(); apps.reload() },
        ) { call("DELETE", path("two-step")) }
    }

    /** Signs the account out everywhere: browsers, apps and compatibility tokens. */
    fun signOutEverywhere() {
        val user = user ?: return
        change(said = AdminDone(SIGNED_OUT_EVERYWHERE, listOf(user.name)), after = { devices.reload(); apps.reload() }) {
            call("DELETE", path("devices"))
        }
    }

    /** Signs [app] out. */
    fun signOutApp(app: UserApp) = change(said = AdminDone(APP_SIGNED_OUT, listOf(app.label)), after = apps::reload) {
        call("DELETE", path("apps", app.id))
    }

    /** Deletes the account; [deleted] once the server did. */
    fun delete() {
        val user = user ?: return
        change {
            call("DELETE", listOf("api", "users", id))
            gone.value = user.name
        }
    }

    /**
     * Sends [body] to the account; what the server answers is shown. A
     * refusal reads the account again, so the page shows what it has.
     * [then] runs after, whatever the answer.
     */
    private fun patch(body: JsonObject, said: String? = null, then: () -> Unit = {}) {
        val user = user ?: return
        var took = false
        change(
            said = said?.let { AdminDone(it, listOf(user.name)) },
            after = {
                if (!took) page.reload()
                then()
            },
        ) {
            val answer = send("PATCH", UserAccount.serializer(), listOf("api", "users", id), body)
            // What the page had, failed reload or not: without it, how many
            // administrators are left isn't known, so it is read again.
            val now = page.state.value.valueOrNull
            if (now != null) page.set(now.copy(user = answer)) else page.reload()
            took = true
        }
    }

    private fun path(vararg rest: Any): List<Any> = listOf("api", "users", id, *rest)

    companion object {
        /** What the page confirms: the words are the screen's. */
        const val SAVED = "saved"
        const val TWO_STEP_OFF = "two_step_off"
        const val TWO_STEP_SIGNED_OUT = "two_step_signed_out"
        const val SIGNED_OUT_EVERYWHERE = "signed_out_everywhere"
        const val APP_SIGNED_OUT = "app_signed_out"

        fun factory(id: Long): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val data = screensGraph().data
                UserViewModel(data.admin, data.session.session, id)
            }
        }
    }
}
