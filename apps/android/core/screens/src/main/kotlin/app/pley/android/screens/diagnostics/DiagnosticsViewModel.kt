package app.pley.android.screens.diagnostics

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.caps.CapsSnapshot
import app.pley.android.caps.DeviceCapabilities
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.UiText
import app.pley.android.model.PleyJson
import app.pley.android.model.caps.ClientCaps
import app.pley.android.player.engine.BufferPlan
import app.pley.android.screens.R
import app.pley.android.screens.screensGraph
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

/** A labelled fact of Diagnostics in English ("Memory tier", "T1"), as the TV's screens show it; [DeviceFact] says it in words. */
data class Fact(val label: String, val value: String)

/** A labelled fact of Diagnostics, in words the screen puts in the language shown: "Memory tier", "T1". */
data class DeviceFact(val label: UiText, val value: UiText)

/** Diagnostics' state. */
data class DiagnosticsUiState(
    /** The device, in English: model, kind, memory and its tier, the player's buffers. */
    val device: List<Fact> = emptyList(),
    /** The same as [device], in words the screen puts in the language shown. */
    val facts: List<DeviceFact> = emptyList(),
    /**
     * What it plays, one line each: decoders (hardware or software, their
     * profiles and limits), HDR types, Dolby Vision profiles, the screen's
     * modes, passthrough formats, quirks. Empty until probed.
     */
    val lines: List<String> = emptyList(),
    /** The report sent with every playback request, as JSON (copyable on phones). */
    val capsJson: String = "",
    val probing: Boolean = true,
    /** The probe failed: its sentence. */
    val error: Throwable? = null,
)

/**
 * Diagnostics (both interfaces): what this device decodes and shows, and
 * why, for when something doesn't play. The capability probe runs if it
 * hasn't (off the main thread).
 */
class DiagnosticsViewModel(
    private val capabilities: DeviceCapabilities,
    profile: DeviceProfile,
    private val compute: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    private val probing = MutableStateFlow(capabilities.snapshot.value == null)
    private val failure = MutableStateFlow<Throwable?>(null)
    private val device = deviceFacts(profile)
    private val facts = factsOf(profile)

    val state: StateFlow<DiagnosticsUiState> = combine(capabilities.snapshot, probing, failure) { snapshot, busy, error ->
        DiagnosticsUiState(
            device = device,
            facts = facts,
            lines = snapshot?.lines.orEmpty(),
            capsJson = snapshot?.let(::capsJson).orEmpty(),
            probing = busy && snapshot == null,
            error = error,
        )
    }.flowOn(compute).stateIn(
        viewModelScope,
        SharingStarted.WhileSubscribed(5_000),
        DiagnosticsUiState(device = device, facts = facts, probing = probing.value),
    )

    init {
        probe()
    }

    /** Probes now unless a probe of this screen and output is saved. */
    fun probe() {
        probing.value = true
        failure.value = null
        viewModelScope.launch(compute) {
            try {
                capabilities.warmUp()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failure.value = e
            } finally {
                probing.value = false
            }
        }
    }

    companion object {
        private val pretty = Json(from = PleyJson) { prettyPrint = true }

        /** The report of [snapshot] as the server receives it, indented. */
        fun capsJson(snapshot: CapsSnapshot): String = pretty.encodeToString(ClientCaps.serializer(), snapshot.report)

        /** What Diagnostics says of the device [profile], in English ([factsOf] in words). */
        fun deviceFacts(profile: DeviceProfile, locale: Locale = Locale.getDefault()): List<Fact> {
            val kind = when {
                profile.fireTv -> "Fire TV"
                profile.form == FormFactor.Tv -> "TV"
                else -> "Phone"
            }
            val ram = String.format(locale, "%.1f GB", profile.totalRamBytes / GB)
            val plan = BufferPlan.of(profile)
            val buffer = buildString {
                append("${plan.minMs / 1000}-${plan.maxMs / 1000} s ahead")
                if (plan.targetBytes > 0) append(", ${plan.targetBytes / MIB} MiB")
                if (plan.backBufferMs > 0) append(", ${plan.backBufferMs / 1000} s behind")
            }
            return listOf(
                Fact("Model", profile.model),
                Fact("Kind", kind),
                Fact("Memory", if (profile.lowRam) "$ram (low-memory device)" else ram),
                Fact("Memory tier", profile.tier.name),
                Fact("Player buffer", buffer),
            )
        }

        /**
         * What Diagnostics says of the device [profile]: its model, its kind,
         * its memory and memory tier, and the player's buffer ("15-50 s
         * ahead, 64 MiB, 30 s behind"). Numbers are written as the language
         * shown writes them.
         */
        fun factsOf(profile: DeviceProfile): List<DeviceFact> {
            val kind = when {
                profile.fireTv -> UiText.raw("Fire TV")
                profile.form == FormFactor.Tv -> UiText.res(R.string.diagnostics_kind_tv)
                else -> UiText.res(R.string.diagnostics_kind_phone)
            }
            val ram = UiText.res(R.string.diagnostics_memory_gb, profile.totalRamBytes / GB)
            val plan = BufferPlan.of(profile)
            val buffer = listOfNotNull(
                UiText.res(R.string.diagnostics_buffer_ahead, plan.minMs / 1000, plan.maxMs / 1000),
                UiText.res(R.string.diagnostics_buffer_bytes, plan.targetBytes / MIB).takeIf { plan.targetBytes > 0 },
                UiText.res(R.string.diagnostics_buffer_behind, plan.backBufferMs / 1000).takeIf { plan.backBufferMs > 0 },
            ).reduce { said, next -> UiText.res(R.string.diagnostics_buffer_and, said, next) }
            return listOf(
                DeviceFact(UiText.res(R.string.diagnostics_fact_model), UiText.raw(profile.model)),
                DeviceFact(UiText.res(R.string.diagnostics_fact_kind), kind),
                DeviceFact(UiText.res(R.string.diagnostics_fact_memory), if (profile.lowRam) UiText.res(R.string.diagnostics_memory_low, ram) else ram),
                DeviceFact(UiText.res(R.string.diagnostics_fact_tier), UiText.raw(profile.tier.name)),
                DeviceFact(UiText.res(R.string.diagnostics_fact_buffer), buffer),
            )
        }

        private const val GB = 1_000_000_000.0
        private const val MIB = 1024 * 1024

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                DiagnosticsViewModel(g.caps.capabilities, g.device)
            }
        }
    }
}
