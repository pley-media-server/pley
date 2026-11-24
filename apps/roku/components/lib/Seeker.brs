' Left and Right in the player, timed as the Android TV player's
' (TvSeeker.kt) and YouTube's TV app have them:
' - A press moves the target 10 s. Presses in a row add up ("+30 s") and
'   seek once, when they settle: Left and Right wait for OK there, 5 s at
'   most after the last key, and fast forward and rewind 400 ms. So a few
'   quick presses don't start the server's conversion again for each.
' - A held key scrubs: the target runs faster the longer the key is held,
'   from 20 ms of video per ms held, doubling every 1.5 s, and nothing
'   seeks while it's held. Let go, it waits where it got to, as presses do.
' A Roku tells a key's press and its release, never repeats: the player
' calls a key held when it isn't let go within skrHoldAfterMs, and moves
' the target on its timer while it is (skrHoldTick).
' The state is plain: { target (-1: no seek coming), asked, holding,
' holdStart, last, presses, waitMs, badgeMs, badge, confirmable }.

function skrStepMs() as integer
    return 10000
end function

' Fast forward and rewind.
function skrJumpMs() as integer
    return 30000
end function

' Fast forward and rewind in a row seek once, this long after the last.
function skrSettleMs() as integer
    return 400
end function

' A seek with Left and Right waits for OK, and goes by itself this long
' after the last key.
function skrWaitMs() as integer
    return 5000
end function

' The seek badge shows this long after the last press.
function skrBadgeMs() as integer
    return 600
end function

' OK this soon after a seek confirms it instead of pausing: a seek far off
' takes a moment to show.
function skrConfirmMs() as integer
    return 2000
end function

' A key not let go this long after its press is held.
function skrHoldAfterMs() as integer
    return 400
end function

' Seeks stop short of the end, so the end isn't reached by seeking.
function skrEndMarginMs() as integer
    return 1000
end function

function skrNew() as object
    return { target: -1, asked: 0, holding: false, holdStart: 0, last: 0, presses: 0, waitMs: skrWaitMs(), badge: false, badgeMs: 0, confirmable: false }
end function

function skrClamp(ms as double, durationMs as integer) as integer
    if durationMs > 0
        last = durationMs - skrEndMarginMs()
        if last < 0 then last = 0
        if ms > last then ms = last
    end if
    if ms < 0 then ms = 0
    return Int(ms)
end function

' A press of Left (direction -1) or Right (+1), or a jump of stepMs (fast
' forward and rewind), seeking waitMs after it unless another comes.
sub skrPress(s as object, direction as integer, stepMs as integer, waitMs as integer, positionMs as integer, durationMs as integer)
    base = s.target
    if base < 0
        base = positionMs
        s.asked = 0
    end if
    ' The badge says what was asked for ("-20 s"), even where the start or
    ' end cuts it short.
    s.asked = s.asked + direction * stepMs
    s.target = skrClamp(base + direction * stepMs, durationMs)
    s.badge = true
    s.badgeMs = s.asked
    s.waitMs = waitMs
    s.presses = s.presses + 1
end sub

' The key is held (nowMs, the player's clock): the target runs on from
' where the press took it.
sub skrHoldStart(s as object, nowMs as integer, positionMs as integer)
    if s.target < 0 then s.target = positionMs
    s.holding = true
    s.holdStart = nowMs
    s.last = nowMs
    s.badge = false
end sub

' The player's timer while the key is held: the target moves by the speed
' held so long, over the time since the last tick (200 ms at most, so a
' stalled frame isn't a jump).
sub skrHoldTick(s as object, direction as integer, nowMs as integer, durationMs as integer)
    if not s.holding then return
    dt = nowMs - s.last
    if dt < 0 then dt = 0
    if dt > 200 then dt = 200
    s.last = nowMs
    rate = skrRateAt(nowMs - s.holdStart, durationMs)
    s.target = skrClamp(s.target + direction * rate * dt, durationMs)
end sub

' How fast a held key scrubs, in ms of video per ms held: faster the longer
' it's held, up to a top that grows with the video (a whole one in about
' 10 s).
function skrRateAt(heldMs as integer, durationMs as integer) as double
    rate = 20# * (2# ^ (heldMs / 1500#))
    top = 300#
    if durationMs / 10000# > top then top = durationMs / 10000#
    if rate > top then rate = top
    return rate
end function

' The key let go: a scrub waits where it got to, as presses do, the 5 s
' from now (it counts as a press). Whether a key was held.
function skrRelease(s as object) as boolean
    if not s.holding then return false
    s.holding = false
    s.waitMs = skrWaitMs()
    s.presses = s.presses + 1
    return true
end function

' waitMs after press number press: where to seek (-1: nothing, as more
' presses came, a key is held or nothing is coming).
function skrSettle(s as object, press as integer) as integer
    if press <> s.presses or s.holding then return -1
    return skrCommit(s)
end function

' OK pressed while a seek is coming, or just after one: as on most TV
' players, OK after Left and Right says "go there", not "pause". Returns
' where to seek now (-1: none), and whether OK was taken by a seek
' (confirmed); if not, it plays or pauses as usual.
function skrConfirm(s as object) as object
    if s.target >= 0
        s.holding = false
        target = skrCommit(s)
        ' Confirmed: the next OK pauses.
        s.confirmable = false
        return { confirmed: true, seekMs: target }
    end if
    confirmed = s.confirmable
    s.confirmable = false
    return { confirmed: confirmed, seekMs: -1 }
end function

' skrConfirmMs after the last seek: OK plays and pauses again.
sub skrSettled(s as object)
    s.confirmable = false
end sub

' Forgets a seek on its way: Back let it go, another seek takes its place,
' or the playback changed under it.
sub skrCancel(s as object)
    s.target = -1
    s.holding = false
    s.badge = false
    s.confirmable = false
end sub

function skrCommit(s as object) as integer
    target = s.target
    if target < 0 then return -1
    s.target = -1
    s.confirmable = true
    return target
end function

' Where the scrubber stands: where a seek coming goes, or the playhead.
function skrAt(s as object, positionMs as integer) as integer
    if s.target >= 0 then return s.target
    return positionMs
end function

' The badge's words, as the Android TV player's: "+30 s", "−10 s".
function skrBadgeText(deltaMs as integer) as string
    seconds = Int(Abs(deltaMs) / 1000)
    if deltaMs < 0 then return t("player.controls.seekBack", { seconds: seconds })
    return t("player.controls.seekForward", { seconds: seconds })
end function
