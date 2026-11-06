' The sleep timer, as the other apps have it: after the minutes chosen, or
' at the end of what plays, the playback pauses. In its last 30 s a notice
' counts down with Keep watching (Keep listening, for music), which adds
' the same time again (or waits for the end of the next title). Minutes
' keep running while paused, and it isn't remembered between playbacks.
' Paused by it for 10 minutes, the video player leaves; music stays
' paused, as on the other apps. Plain state: the player asks it once a
' second with the clock's milliseconds.

' What the timer is set to, in order: [{ label, minutes }], minutes 0 for
' Off and -1 for the end of what plays (kind: "episode", "movie", "track"
' or another).
function sleepChoices(kind as string) as object
    choices = [{ label: t("player.sleep.off"), minutes: 0 }]
    for each minutes in [15, 30, 45, 60, 90]
        choices.Push({ label: t("player.sleep.minutes", { minutes: minutes }), minutes: minutes })
    end for
    choices.Push({ label: sleepEndLabel(kind), minutes: -1 })
    return choices
end function

function sleepEndLabel(kind as string) as string
    if kind = "episode" or kind = "movie" or kind = "track" then return t("player.sleep.end." + kind)
    return t("player.sleep.end.video")
end function

' A timer: { minutes (0 off, -1 the end of what plays), deadlineMs, spared
' (what Keep watching let play past its end, "" for none) }.
function sleepNew(minutes as integer, nowMs as double) as object
    timer = { minutes: minutes, deadlineMs: 0#, spared: "" }
    if minutes > 0 then timer.deadlineMs = nowMs + minutes * 60000#
    return timer
end function

' Keep watching: the minutes chosen again, or until the end of the next
' title (itemKey, what plays now, plays to its end).
sub sleepExtend(timer as object, itemKey as string)
    if timer.minutes > 0
        timer.deadlineMs = timer.deadlineMs + timer.minutes * 60000#
    else if timer.minutes = -1
        timer.spared = itemKey
    end if
end sub

' What the timer says at nowMs: { fire (pause now), secondsLeft (the
' notice's countdown, its last 30 s; -1 otherwise), minutesLeft (rounded
' up, for the menu; -1 when off or not known) }. itemLeftMs: what is left
' of what plays (-1 not known); itemKey: what plays.
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
        ' The title Keep watching spared ended: the timer waits for this one.
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

' What the menu says of it: "Off", "23 min left", "End of this episode".
function sleepLabel(timer as object, state as object, kind as string) as string
    if timer.minutes = 0 then return t("player.sleep.off")
    if timer.minutes = -1 and state.secondsLeft < 0 then return sleepEndLabel(kind)
    if state.secondsLeft >= 0 then return t("player.sleep.secondsLeft", { count: state.secondsLeft })
    if state.minutesLeft > 0 then return t("player.sleep.minutesLeft", { count: state.minutesLeft })
    return sleepEndLabel(kind)
end function

' The choice after minutes, in sleepChoices' order, Off after the last:
' what a button that steps through them sets next.
function sleepNextMinutes(minutes as integer) as integer
    choices = sleepChoices("")
    for i = 0 to choices.Count() - 2
        if choices[i].minutes = minutes then return choices[i + 1].minutes
    end for
    return 0
end function

' What Now Playing's sleep timer button says: "Sleep timer" while off,
' "Keep listening" in its last 30 s, else what is left ("Sleep timer: 23
' min left", "Sleep timer: End of this track").
function sleepButtonLabel(timer as object, state as object) as string
    if timer.minutes = 0 then return t("player.sleep.title")
    if state.secondsLeft >= 0 then return t("player.sleep.keepListening")
    return t("player.sleep.titleNow", { state: sleepLabel(timer, state, "track") })
end function

' How long the timer's pause lasts before the player leaves.
function sleepLeaveAfterMs() as integer
    return 600000
end function
