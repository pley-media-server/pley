sub test_seeker_presses_add_up_and_wait()
    s = skrNew()
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 60000, 600000)
    tkEqual(s.target, 70000)
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 61000, 600000)
    tkEqual(s.target, 80000, "from the target, not the playhead that moved on")
    tkEqual(s.badgeMs, 20000)
    tkEqual(skrBadgeText(s.badgeMs), "+20 s")
    tkEqual(s.waitMs, 5000, "Left and Right wait for OK")
    tkEqual(skrSettle(s, s.presses - 1), -1, "a press came since")
    tkEqual(skrSettle(s, s.presses), 80000)
    tkEqual(s.target, -1)
end sub

sub test_seeker_jumps_settle_soon()
    s = skrNew()
    skrPress(s, -1, skrJumpMs(), skrSettleMs(), 600000, 3600000)
    tkEqual(s.target, 570000)
    tkEqual(s.waitMs, 400)
    tkEqual(skrBadgeText(s.badgeMs), "−30 s")
end sub

sub test_seeker_stays_within_the_video()
    s = skrNew()
    skrPress(s, -1, skrStepMs(), skrWaitMs(), 4000, 600000)
    tkEqual(s.target, 0)
    tkEqual(skrBadgeText(s.badgeMs), "−10 s", "the badge says what was asked for")
    s = skrNew()
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 595000, 600000)
    tkEqual(s.target, 599000, "short of the end")
end sub

sub test_seeker_ok_confirms_a_seek()
    s = skrNew()
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 0, 600000)
    result = skrConfirm(s)
    tkTrue(result.confirmed)
    tkEqual(result.seekMs, 10000)
    result = skrConfirm(s)
    tkFalse(result.confirmed, "the next OK pauses")
    tkEqual(result.seekMs, -1)
end sub

sub test_seeker_ok_just_after_a_seek_confirms_it()
    s = skrNew()
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 0, 600000)
    tkEqual(skrSettle(s, s.presses), 10000)
    tkTrue(skrConfirm(s).confirmed, "within the 2 s after")
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 10000, 600000)
    skrSettle(s, s.presses)
    skrSettled(s)
    tkFalse(skrConfirm(s).confirmed, "after them, OK plays and pauses")
end sub

sub test_seeker_a_held_key_scrubs_faster()
    s = skrNew()
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 0, 7200000)
    skrHoldStart(s, 1000, 0)
    tkFalse(s.badge, "no badge while scrubbing")
    skrHoldTick(s, 1, 1100, 7200000)
    tkEqual(s.target, 12094, "about 20 ms a ms held (2^(0.1 / 1.5) × 20 at 0.1 s)")
    tkEqual(skrSettle(s, s.presses), -1, "nothing seeks while held")
    tkTrue(skrRelease(s))
    tkEqual(s.waitMs, 5000, "let go, it waits for OK")
    tkEqual(skrSettle(s, s.presses), 12094)
end sub

sub test_seeker_rate_tops_out_with_the_video()
    tkEqual(Int(skrRateAt(0, 600000)), 20)
    tkEqual(Int(skrRateAt(1500, 600000)), 40)
    tkEqual(Int(skrRateAt(60000, 600000)), 300, "a short video's top")
    tkEqual(Int(skrRateAt(60000, 7200000)), 720, "a whole long video in about 10 s")
end sub

sub test_seeker_a_stalled_tick_isnt_a_jump()
    s = skrNew()
    skrHoldStart(s, 0, 50000)
    skrHoldTick(s, -1, 5000, 600000)
    tkEqual(s.target, 9682, "200 ms at most at 5 s held: 50 000 - 201.6 × 200")
end sub

sub test_seeker_cancel_forgets()
    s = skrNew()
    skrPress(s, 1, skrStepMs(), skrWaitMs(), 0, 600000)
    skrCancel(s)
    tkEqual(s.target, -1)
    tkEqual(skrAt(s, 1234), 1234)
    tkFalse(skrConfirm(s).confirmed)
end sub
