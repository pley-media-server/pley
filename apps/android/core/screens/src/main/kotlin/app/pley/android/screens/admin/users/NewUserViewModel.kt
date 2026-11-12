package app.pley.android.screens.admin.users

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.model.Library
import app.pley.android.model.RequestAccess
import app.pley.android.model.PleyJson
import app.pley.android.model.admin.LabelUse
import app.pley.android.model.admin.MetadataKeys
import app.pley.android.model.admin.NewUserAccount
import app.pley.android.model.admin.RatingSystems
import app.pley.android.model.admin.UserAccount
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.builtins.ListSerializer

/** A regular account signs in with a password; a managed profile is switched to from a home member's. */
enum class AccountKind { Regular, Managed }

/**
 * What a new account is made with, as the web's Add user dialog asks it,
 * but for its role: the web app makes administrators.
 */
data class NewUserForm(
    val kind: AccountKind = AccountKind.Regular,
    val username: String = "",
    val displayName: String = "",
    val password: String = "",
    val home: Boolean = true,
    val access: AccessChoice = AccessChoice(),
    val pin: String = "",
    /** Requests as chosen; `null` until then, which follows [requestsByDefault]. */
    val requestChoice: RequestAccess? = null,
) {
    val managed: Boolean get() = kind == AccountKind.Managed

    /** Requests as the page shows them. */
    val requests: RequestAccess get() = requestChoice ?: requestsByDefault(managed, access.maxAge)
}

/** What keeps a [NewUserForm] from being sent, as the web says it before asking the server. */
sealed interface NewUserProblem {
    data object EnterUsername : NewUserProblem

    data class Password(val problem: PasswordProblem) : NewUserProblem

    data object PinDigits : NewUserProblem

    data object PickLibrary : NewUserProblem

    /** A label both always and never shown. */
    data class LabelsInBoth(val labels: List<String>) : NewUserProblem
}

/** [form]'s first problem, or `null`. */
fun problemOf(form: NewUserForm): NewUserProblem? {
    if (form.username.isBlank()) return NewUserProblem.EnterUsername
    if (!form.managed) passwordProblem(form.password, form.username)?.let { return NewUserProblem.Password(it) }
    if (form.pin.isNotEmpty() && !pinOk(form.pin)) return NewUserProblem.PinDigits
    if (form.access.noLibrary) return NewUserProblem.PickLibrary
    form.access.labelsInBoth.takeIf { it.isNotEmpty() }?.let { return NewUserProblem.LabelsInBoth(it) }
    return null
}

/**
 * Adding a user (the web's Add user dialog): a regular account or a
 * managed profile, with its fields, sent together by one button.
 */
class NewUserViewModel(admin: ServerAdmin) : AdminViewModel(admin) {
    val libraries = resource { get(ListSerializer(Library.serializer()), LIBRARIES) }
    val ratingSystems = resource { get(RatingSystems.serializer(), RATING_SYSTEMS) }
    val labels = resource { get(ListSerializer(LabelUse.serializer()), LABELS) }
    val metadataKeys = resource { get(MetadataKeys.serializer(), SYSTEM) }

    private val formState = MutableStateFlow(NewUserForm())
    val form: StateFlow<NewUserForm> = formState.asStateFlow()

    private val made = MutableStateFlow<UserAccount?>(null)

    /** The account made: the page says so and goes back. */
    val created: StateFlow<UserAccount?> = made.asStateFlow()

    /** Changes the form; choosing another kind forgets the requests chosen, as the web does. */
    fun edit(how: (NewUserForm) -> NewUserForm) = formState.update { before ->
        val after = how(before)
        if (after.kind != before.kind) after.copy(requestChoice = null) else after
    }

    /** Sends the form, unless [problemOf] it says what's wrong (the page shows it). */
    fun create(): NewUserProblem? {
        problemOf(formState.value)?.let { return it }
        val form = formState.value
        val name = form.displayName.trim()
        val body = NewUserAccount(
            username = form.username.trim(),
            password = form.password.takeIf { !form.managed },
            displayName = name.ifEmpty { null },
            managed = form.managed,
            home = form.home,
            allLibraries = form.access.allLibraries,
            libraryIds = form.access.libraryIds,
            // A limit that follows a birthday is worked out by the server.
            maxAge = form.access.maxAge.takeIf { form.access.birthMonth == null },
            maxAgeSystem = form.access.maxAgeSystem,
            birthMonth = form.access.birthMonth,
            showLabels = form.access.showLabels.ifEmpty { null },
            hideLabels = form.access.hideLabels.ifEmpty { null },
            pin = form.pin.ifEmpty { null },
            requestAccess = form.requests,
        )
        // The page says so as it goes back ([created]), as a confirmation could arrive after it left.
        change {
            made.value = send("POST", UserAccount.serializer(), USERS, PleyJson.encodeToJsonElement(NewUserAccount.serializer(), body))
        }
        return null
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { NewUserViewModel(screensGraph().data.admin) }
        }
    }
}
