' The music's sleep timer, as the other apps have it: after the minutes
' chosen, or at the end of the track, the music pauses. In its last 30 s
' Now Playing counts down and its button says Keep listening, which adds
' the same time again (or waits for the end of the next track). Minutes
' keep running while paused, and it isn't remembered between playbacks.
' Paused by it, the music stays paused, as on the other apps. Plain state:
' the music player asks it once a second with the clock's milliseconds.

' A timer: { minutes (0 off, -1 the end of the track), deadlineMs, spared
' (the track Keep listening let play past its end, "" for none) }.
function sleepNew(minutes as integer, nowMs as double) as object
    timer = { minutes: minutes, deadlineMs: 0#, spared: "" }
    if minutes > 0 then timer.deadlineMs = nowMs + minutes * 60000#
    return timer
end function

' Keep listening: the minutes chosen again, or until the end of the next
' track (itemKey, the one playing now, plays to its end).
sub sleepExtend(timer as object, itemKey as string)
    if timer.minutes > 0
        timer.deadlineMs = timer.deadlineMs + timer.minutes * 60000#
    else if timer.minutes = -1
        timer.spared = itemKey
    end if
end sub

' What the timer says at nowMs: { fire (pause now), secondsLeft (the
' countdown of its last 30 s; -1 otherwise), minutesLeft (rounded up, for
' the button; -1 when off or not known) }. itemLeftMs: what is left of the
' track (-1 not known); itemKey: the track playing.
function sleepState(timer as object, nowMs as double, itemLeftMs as double, itemKey as string) as object
    state = { fire: false, secondsLeft: -1, minutesLeft: -1 }
    if timer.minutes = 0 then return state
    if timer.minutes > 0
        left = timer.deadlineMs - nowMs
        if left <= 0
            state.fire = true
            return state
        end if
    else
        ' The track Keep listening spared ended: the timer waits for this one.
        if timer.spared <> "" and timer.spared <> itemKey then timer.spared = ""
        if timer.spared = itemKey or itemLeftMs <= 0 then return state
        left = itemLeftMs
    end if
    if left <= 30000
        state.secondsLeft = -Int(-left / 1000)
        state.minutesLeft = 1
    else
        state.minutesLeft = -Int(-left / 60000)
    end if
    return state
end function

' Whether the timer ends with itemKey, which played to its end: nothing
' plays after it.
function sleepEndsWith(timer as object, itemKey as string) as boolean
    return timer.minutes = -1 and timer.spared <> itemKey
end function

' What is left of a timer that is set: "23 min left", "End of this track".
function sleepLabel(timer as object, state as object) as string
    if timer.minutes > 0 and state.minutesLeft > 0 then return t("player.sleep.minutesLeft", { count: state.minutesLeft })
    return t("player.sleep.end.track")
end function

' The choice after minutes, in the other apps' order (15 to 90 minutes,
' then the end of the track), Off after the last: what a button that steps
' through them sets next.
function sleepNextMinutes(minutes as integer) as integer
    choices = [0, 15, 30, 45, 60, 90, -1]
    for i = 0 to choices.Count() - 2
        if choices[i] = minutes then return choices[i + 1]
    end for
    return 0
end function

' What Now Playing's sleep timer button says: "Sleep timer" while off,
' "Keep listening" in its last 30 s, else what is left ("Sleep timer: 23
' min left", "Sleep timer: End of this track").
function sleepButtonLabel(timer as object, state as object) as string
    if timer.minutes = 0 then return t("player.sleep.title")
    if state.secondsLeft >= 0 then return t("player.sleep.keepListening")
    return t("player.sleep.titleNow", { state: sleepLabel(timer, state) })
end function
