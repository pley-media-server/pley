package app.pley.android.player.sleep

import androidx.compose.runtime.Immutable
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What the sleep timer is set to. Not remembered after the music stops. */
sealed interface SleepChoice {
    data object Off : SleepChoice

    data class Minutes(val minutes: Int) : SleepChoice

    /** The end of the track playing. */
    data object EndOfItem : SleepChoice

    companion object {
        /** The menu's choices, in order. */
        val ALL: List<SleepChoice> = listOf(Off, Minutes(15), Minutes(30), Minutes(45), Minutes(60), Minutes(90), EndOfItem)
    }
}

/** Where the sleep timer stands. */
@Immutable
data class SleepState(
    val choice: SleepChoice = SleepChoice.Off,
    /** Seconds before it pauses, during the notice (its last 30 s); `null` otherwise. */
    val secondsLeft: Int? = null,
    /** Minutes before it pauses, rounded up, for the menu; `null` when off or unknown. */
    val minutesLeft: Int? = null,
)

/**
 * The sleep timer of music: after the minutes chosen, or at the end of
 * what plays, it pauses ([onFire]). 30 s before, [state] says so once a
 * second for a notice; [extend] ("Keep listening") adds the same time
 * again. Time comes from [now] (monotonic ms) and [delay], so tests run
 * it on virtual time. Main thread only.
 */
class SleepTimer(
    private val scope: CoroutineScope,
    private val now: () -> Long,
    /** How long the item playing has left, in ms of playback; `null` while unknown. */
    private val itemLeftMs: () -> Long?,
    /** Tells items apart: the one playing now. */
    private val itemKey: () -> Any?,
    private val onFire: () -> Unit,
) {
    private val mutable = MutableStateFlow(SleepState())
    val state: StateFlow<SleepState> = mutable.asStateFlow()

    private var job: Job? = null

    /** When a timer of minutes ends, on [now]'s clock. */
    private var deadline = 0L

    /** "Keep listening" at the end of this item: the timer waits for the next one's end. */
    private var spared: Any? = null

    fun set(choice: SleepChoice) {
        job?.cancel()
        job = null
        spared = null
        mutable.value = SleepState(choice)
        when (choice) {
            SleepChoice.Off -> return
            is SleepChoice.Minutes -> deadline = now() + choice.minutes * MINUTE_MS
            SleepChoice.EndOfItem -> Unit
        }
        run()
    }

    /** Adds the same time again: the minutes chosen, or until the end of the next item. */
    fun extend() {
        when (val choice = mutable.value.choice) {
            SleepChoice.Off -> return
            is SleepChoice.Minutes -> deadline += choice.minutes * MINUTE_MS
            SleepChoice.EndOfItem -> spared = itemKey()
        }
        mutable.update { it.copy(secondsLeft = null) }
        run()
    }

    /** Whether the timer ends with the item playing now: what comes next waits. */
    fun endsWith(key: Any?): Boolean = mutable.value.choice == SleepChoice.EndOfItem && key != spared

    /**
     * The item [key] played to its end: true when that is the timer's end,
     * which then fires (the caller holds what comes next).
     */
    fun itemEnded(key: Any?): Boolean {
        if (!endsWith(key)) return false
        fire()
        return true
    }

    fun cancel() {
        job?.cancel()
        job = null
        mutable.value = SleepState()
    }

    private fun run() {
        job?.cancel()
        job = scope.launch {
            while (true) {
                val choice = mutable.value.choice
                val key = itemKey()
                val left = when (choice) {
                    is SleepChoice.Minutes -> deadline - now()
                    SleepChoice.EndOfItem -> if (key == spared) null else itemLeftMs()
                    SleepChoice.Off -> return@launch
                }
                if (spared != null && key != spared) spared = null
                when {
                    left == null -> publish(null, null)
                    choice is SleepChoice.Minutes && left <= 0 -> {
                        fire()
                        return@launch
                    }
                    // The end of the item fires through itemEnded, when it truly ends.
                    left <= 0 -> publish(null, null)
                    left <= NOTICE_MS -> publish(((left + SECOND_MS - 1) / SECOND_MS).toInt(), 1)
                    else -> publish(null, ((left + MINUTE_MS - 1) / MINUTE_MS).toInt())
                }
                delay(wait(choice, left))
            }
        }
    }

    /** Until the next second during the notice; else until the notice or the next minute, polling an item's end each second. */
    private fun wait(choice: SleepChoice, left: Long?): Long = when {
        left == null || left <= 0 || choice == SleepChoice.EndOfItem -> SECOND_MS
        left <= NOTICE_MS -> (left % SECOND_MS).takeIf { it > 0 } ?: SECOND_MS
        else -> minOf(left - NOTICE_MS, (left % MINUTE_MS).takeIf { it > 0 } ?: MINUTE_MS)
    }

    private fun publish(secondsLeft: Int?, minutesLeft: Int?) {
        mutable.update { it.copy(secondsLeft = secondsLeft, minutesLeft = minutesLeft) }
    }

    private fun fire() {
        job?.cancel()
        job = null
        spared = null
        mutable.value = SleepState()
        onFire()
    }

    companion object {
        /** The notice shows this long before the timer pauses. */
        const val NOTICE_MS = 30_000L

        private const val SECOND_MS = 1_000L
        private const val MINUTE_MS = 60_000L
    }
}
