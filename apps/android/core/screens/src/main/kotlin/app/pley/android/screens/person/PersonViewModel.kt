package app.pley.android.screens.person

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.UiText
import app.pley.android.data.DataGraph
import app.pley.android.data.Loadable
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PersonDetail
import app.pley.android.model.PersonRole
import app.pley.android.network.ApiException
import app.pley.android.screens.R
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update

/** A person's page (apps/web/src/pages/PersonPage.tsx). */
sealed interface PersonUiState {
    data object Loading : PersonUiState

    /** "Person not found": no such person, or nothing of theirs this profile may see (404). */
    data object NotFound : PersonUiState

    data class Failed(val error: Throwable) : PersonUiState

    data class Ready(val page: PersonPage, val status: Loadable<Unit>) : PersonUiState
}

/** An actor's or director's page: what they are in, in this server's libraries. */
data class PersonPage(
    val id: Long,
    val name: String,
    /** The photo's address as the server gave it; `null` without one. */
    val photoUrl: String?,
    val origin: String,
    /** "3 movies", "2 shows", for "3 movies · 2 shows in your libraries"; empty when there are none. */
    val countTexts: List<UiText>,
    /** Movies, newest first, with their roles: "2019 · as Neo". */
    val movies: List<CreditTile>,
    val shows: List<CreditTile>,
    /** The same titles by role (the TV's rows): acting first, then directing, creating, writing, producing. */
    val roles: List<RoleRow>,
)

/**
 * A title someone is credited in, with its caption's second line: the
 * year, then their [roles] in words ("2019 · as Neo").
 */
data class CreditTile(val item: ItemSummary, val roles: List<UiText>)

/** Titles of one role. */
data class RoleRow(val role: CreditRole, val credits: List<CreditTile>)

/** The person's page, from what was saved first and the server's answer after. */
@OptIn(ExperimentalCoroutinesApi::class)
class PersonViewModel(
    private val data: DataGraph,
    val personId: Long,
    compute: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    private val retries = MutableStateFlow(0)

    val state: StateFlow<PersonUiState> = retries
        .flatMapLatest {
            combine(data.people.person(personId), data.servers.active.map { it.orEmpty() }.distinctUntilChanged()) { load, origin ->
                val person = load.valueOrNull
                when {
                    person != null -> PersonUiState.Ready(personPage(person, origin), load.map { })
                    load is Loadable.Failed && (load.error as? ApiException)?.status == 404 -> PersonUiState.NotFound
                    load is Loadable.Failed -> PersonUiState.Failed(load.error)
                    else -> PersonUiState.Loading
                }
            }
        }
        .flowOn(compute)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), PersonUiState.Loading)

    fun retry() {
        retries.update { it + 1 }
    }

    companion object {
        fun factory(personId: Long): ViewModelProvider.Factory = viewModelFactory {
            initializer { PersonViewModel(screensGraph().data, personId) }
        }
    }
}

/** The page of [person], whose pictures are on [origin]. */
fun personPage(person: PersonDetail, origin: String): PersonPage {
    val credits = person.credits.distinctBy { it.item.id }
    val movies = credits.filter { it.item.kind == ItemKind.Movie }
    val shows = credits.filter { it.item.kind == ItemKind.Show }
    val countTexts = listOfNotNull(
        movies.size.takeIf { it > 0 }?.let { UiText.plural(R.plurals.person_count_movies, it) },
        shows.size.takeIf { it > 0 }?.let { UiText.plural(R.plurals.person_count_shows, it) },
    )
    val roles = ROLE_ORDER.mapNotNull { role ->
        val titles = credits.filter { c -> c.roles.any { it.role == role } }
        if (titles.isEmpty()) {
            null
        } else {
            RoleRow(role, titles.map { c -> tile(c.item, c.roles.filter { it.role == role }) })
        }
    }
    return PersonPage(
        id = person.id,
        name = person.name,
        photoUrl = person.photoUrl?.takeIf { it.isNotEmpty() },
        origin = origin,
        countTexts = countTexts,
        movies = movies.map { tile(it.item, it.roles) },
        shows = shows.map { tile(it.item, it.roles) },
        roles = roles,
    )
}

private val ROLE_ORDER = listOf(CreditRole.Actor, CreditRole.Director, CreditRole.Creator, CreditRole.Writer, CreditRole.Producer)

private fun tile(item: ItemSummary, roles: List<PersonRole>) = CreditTile(item, roleWords(roles))

private val ROLE_WORDS = mapOf(
    CreditRole.Actor to R.string.person_role_actor,
    CreditRole.Creator to R.string.person_role_creator,
    CreditRole.Director to R.string.person_role_director,
    CreditRole.Writer to R.string.person_role_writer,
    CreditRole.Producer to R.string.person_role_producer,
)

/** Each of [roles] in words, as the web says it: "as Neo", "Director", "Guest star (3 episodes)". */
fun roleWords(roles: List<PersonRole>): List<UiText> = roles.map { r ->
    val episodes = r.episodes ?: 0
    val base = if (r.role == CreditRole.Actor) {
        r.character?.takeIf { it.isNotEmpty() }?.let { UiText.res(R.string.person_role_as, it) }
            ?: UiText.res(if (episodes > 0) R.string.person_role_guest_star else R.string.person_role_actor)
    } else {
        UiText.res(ROLE_WORDS[r.role] ?: R.string.person_role_crew)
    }
    if (episodes > 0) UiText.plural(R.plurals.person_role_episodes, episodes, base, episodes) else base
}
