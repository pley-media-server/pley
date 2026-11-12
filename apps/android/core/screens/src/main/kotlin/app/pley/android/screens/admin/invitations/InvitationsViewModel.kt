package app.pley.android.screens.admin.invitations

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.data.Session
import app.pley.android.model.Library
import app.pley.android.model.PleyJson
import app.pley.android.model.admin.Invitation
import app.pley.android.model.admin.LabelUse
import app.pley.android.model.admin.MetadataKeys
import app.pley.android.model.admin.NewInvitation
import app.pley.android.model.admin.RatingSystems
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.admin.users.AccessChoice
import app.pley.android.screens.admin.users.LABELS
import app.pley.android.screens.admin.users.LIBRARIES
import app.pley.android.screens.admin.users.RATING_SYSTEMS
import app.pley.android.screens.admin.users.SYSTEM
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.builtins.ListSerializer

/** Where an invitation stands: it still makes accounts, every use is spent, or it is past its date. */
enum class InvitationState { Active, Used, Expired }

/** [invitation]'s state at [now] (unix seconds), as the web decides it: spent first, then expired. */
fun stateOf(invitation: Invitation, now: Long): InvitationState = when {
    invitation.usesLeft <= 0 -> InvitationState.Used
    invitation.expiresAt.let { it != null && it <= now } -> InvitationState.Expired
    else -> InvitationState.Active
}

/** The link to send: the server's address and `/invite/<code>`, where the web's sign-up page takes it. */
fun invitationLink(server: String, code: String): String = server.trimEnd('/') + "/invite/" + code

/**
 * Invitations (the web's Settings → Invitations): the links that make an
 * account, newest first, with their state, and deleting one. A new one is
 * made on its own page ([NewInvitationViewModel]).
 */
class InvitationsViewModel(
    admin: ServerAdmin,
    private val session: StateFlow<Session?>,
    private val clock: () -> Long = { System.currentTimeMillis() / 1000 },
) : AdminViewModel(admin) {
    val invitations = resource { get(ListSerializer(Invitation.serializer()), INVITES) }
    val libraries = resource { get(ListSerializer(Library.serializer()), LIBRARIES) }

    /** What the rows' age limits are shown in ("Up to PG-13"). */
    val ratingSystems = resource { get(RatingSystems.serializer(), RATING_SYSTEMS) }

    private val shownAt = MutableStateFlow(clock())

    /** When the page last showed: "expires" or "expired" is decided once per visit, as the web does. */
    val now: StateFlow<Long> = shownAt.asStateFlow()

    /** [invitation]'s link; `null` while the server isn't known. */
    fun link(invitation: Invitation): String? = session.value?.issuer?.let { invitationLink(it, invitation.code) }

    fun delete(invitation: Invitation) = change(said = AdminDone(DELETED), after = invitations::reload) {
        call("DELETE", listOf("api", "invites", invitation.code))
    }

    /** The page shows again after making one: it is read, as of now. */
    fun shownAgain() {
        shownAt.value = clock()
        invitations.reload()
    }

    companion object {
        const val DELETED = "deleted"

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val data = screensGraph().data
                InvitationsViewModel(data.admin, data.session.session)
            }
        }
    }
}

/** How long a new link lasts, in days, as the web offers; 0 is never. */
val EXPIRY_DAYS: List<Int> = listOf(1, 7, 30, 0)

/** The most accounts one link makes, as the server allows. */
const val MAX_USES = 1000

/** A new invitation, as the web's New invitation dialog asks it. */
data class NewInvitationForm(
    val days: Int = 7,
    val uses: Int = 1,
    val note: String = "",
    val access: AccessChoice = AccessChoice(),
)

/** What keeps a [NewInvitationForm] from being sent, as the web says it before asking the server. */
enum class NewInvitationProblem { PickLibrary, AtLeastOne, LabelsInBoth }

fun problemOf(form: NewInvitationForm): NewInvitationProblem? = when {
    form.access.noLibrary -> NewInvitationProblem.PickLibrary
    form.uses < 1 -> NewInvitationProblem.AtLeastOne
    form.access.labelsInBoth.isNotEmpty() -> NewInvitationProblem.LabelsInBoth
    else -> null
}

/**
 * Making an invitation: how long it lasts, how many accounts it makes, a
 * note, and what they see, sent together; then its link, to copy or share.
 */
class NewInvitationViewModel(admin: ServerAdmin, private val session: StateFlow<Session?>) : AdminViewModel(admin) {
    val libraries = resource { get(ListSerializer(Library.serializer()), LIBRARIES) }
    val ratingSystems = resource { get(RatingSystems.serializer(), RATING_SYSTEMS) }
    val labels = resource { get(ListSerializer(LabelUse.serializer()), LABELS) }
    val metadataKeys = resource { get(MetadataKeys.serializer(), SYSTEM) }

    private val formState = MutableStateFlow(NewInvitationForm())
    val form: StateFlow<NewInvitationForm> = formState.asStateFlow()

    private val made = MutableStateFlow<Invitation?>(null)

    /** The invitation made, whose link the page shows. */
    val created: StateFlow<Invitation?> = made.asStateFlow()

    fun edit(how: (NewInvitationForm) -> NewInvitationForm) = formState.update(how)

    /** Sends the form, unless [problemOf] it says what's wrong (the page shows it). */
    fun create(): NewInvitationProblem? {
        val form = formState.value
        problemOf(form)?.let { return it }
        val body = NewInvitation(
            expiresInDays = form.days,
            uses = form.uses.coerceAtMost(MAX_USES),
            allLibraries = form.access.allLibraries,
            libraryIds = form.access.libraryIds,
            maxAge = form.access.maxAge,
            maxAgeSystem = form.access.maxAgeSystem,
            showLabels = form.access.showLabels.ifEmpty { null },
            hideLabels = form.access.hideLabels.ifEmpty { null },
            note = form.note.trim().ifEmpty { null },
        )
        change {
            made.value = send("POST", Invitation.serializer(), INVITES, PleyJson.encodeToJsonElement(NewInvitation.serializer(), body))
        }
        return null
    }

    /** The link of the invitation made; `null` before, or while the server isn't known. */
    fun link(): String? {
        val code = made.value?.code ?: return null
        return session.value?.issuer?.let { invitationLink(it, code) }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val data = screensGraph().data
                NewInvitationViewModel(data.admin, data.session.session)
            }
        }
    }
}

private val INVITES = listOf("api", "invites")
