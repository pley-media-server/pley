package app.pley.android.screens.admin.livetv

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.ServerAdmin
import app.pley.android.model.PleyJson
import app.pley.android.model.admin.LineupGroup
import app.pley.android.model.admin.LineupPatch
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminViewModel
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.serialization.builtins.ListSerializer

/**
 * Live TV's Groups, as the web's tab: whole groups, and whole regions of
 * them, shown or hidden at once, found by name, the regions opened as
 * people look into them.
 */
class LineupGroupsViewModel(admin: ServerAdmin) : AdminViewModel(admin) {
    val groups = resource { get(ListSerializer(LineupGroup.serializer()), listOf("api", "live", "lineup", "groups")) }

    private val queryNow = MutableStateFlow("")
    private val shownNow = MutableStateFlow(GroupsShown.All)

    /** What the groups are found by, and which are listed. */
    val query: StateFlow<String> = queryNow.asStateFlow()
    val shown: StateFlow<GroupsShown> = shownNow.asStateFlow()

    /** The groups listed, by region; `null` until they are read. Made off the main thread: a provider has thousands. */
    val sections: StateFlow<List<GroupSection>?> =
        combine(groups.state, queryNow, shownNow) { load, query, shown -> load.valueOrNull?.let { groupSections(it, query, shown) } }
            .flowOn(Dispatchers.Default)
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val openNow = MutableStateFlow<Set<String>>(emptySet())

    /** The regions opened, their groups listed under them. */
    val open: StateFlow<Set<String>> = openNow.asStateFlow()

    fun find(query: String) {
        queryNow.value = query
    }

    fun show(which: GroupsShown) {
        shownNow.value = which
    }

    fun toggle(region: String) = openNow.update { if (region in it) it - region else it + region }

    /** Hides or shows every channel of [section]'s groups. */
    fun setHidden(section: GroupSection, hidden: Boolean) {
        val region = section.region
        val said = when {
            region != null -> AdminDone(if (hidden) REGION_HIDDEN else REGION_SHOWN, listOf(region))
            else -> doneFor(section.groups.single(), hidden)
        }
        setHidden(section.groups.map { it.name }, hidden, said)
    }

    /** Hides or shows every channel of [group]. */
    fun setHidden(group: LineupGroup, hidden: Boolean) = setHidden(listOf(group.name), hidden, doneFor(group, hidden))

    private fun doneFor(group: LineupGroup, hidden: Boolean) = when {
        group.name.isEmpty() -> AdminDone(if (hidden) UNNAMED_HIDDEN else UNNAMED_SHOWN)
        else -> AdminDone(if (hidden) GROUP_HIDDEN else GROUP_SHOWN, listOf(group.name))
    }

    private fun setHidden(names: List<String>, hidden: Boolean, said: AdminDone) {
        // Shown at once, as the switches say; what the server has is read after.
        groups.state.value.valueOrNull?.let { all ->
            groups.set(all.map { g -> if (g.name in names) g.copy(hidden = if (hidden) g.count else 0) else g })
        }
        change(said = said, after = groups::reload) {
            call("PATCH", listOf("api", "live", "lineup"), PleyJson.encodeToJsonElement(LineupPatch.serializer(), LineupPatch(groups = names, hidden = hidden)))
        }
    }

    companion object {
        const val REGION_HIDDEN = "livetv.groups.region.hidden"
        const val REGION_SHOWN = "livetv.groups.region.shown"
        const val GROUP_HIDDEN = "livetv.groups.group.hidden"
        const val GROUP_SHOWN = "livetv.groups.group.shown"
        const val UNNAMED_HIDDEN = "livetv.groups.unnamed.hidden"
        const val UNNAMED_SHOWN = "livetv.groups.unnamed.shown"

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer { LineupGroupsViewModel(screensGraph().data.admin) }
        }
    }
}
