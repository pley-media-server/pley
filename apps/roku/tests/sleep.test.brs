sub test_sleep_choices_as_the_other_apps()
    labels = []
    for each choice in sleepChoices("episode")
        labels.Push(choice.label)
    end for
    tkEqual(labels, ["Off", "15 min", "30 min", "45 min", "60 min", "90 min", "End of this episode"])
    tkEqual(sleepChoices("movie").Peek().label, "End of this movie")
    tkEqual(sleepChoices("track").Peek().label, "End of this track")
    tkEqual(sleepChoices("extra").Peek().label, "End of this video")
    tkEqual(sleepChoices("movie").Peek().minutes, -1)
end sub

sub test_sleep_minutes_count_down_then_fire()
    timer = sleepNew(15, 1000#)
    state = sleepState(timer, 1000#, 5000#, "a")
    tkFalse(state.fire)
    tkEqual(state.minutesLeft, 15)
    tkEqual(state.secondsLeft, -1)
    tkEqual(sleepLabel(timer, state, "movie"), "15 min left")
    state = sleepState(timer, 1000# + 14.5 * 60000#, 5000#, "a")
    tkEqual(state.minutesLeft, 1, "rounded up")
    tkEqual(state.secondsLeft, 30, "the notice: its last 30 s")
    tkEqual(sleepLabel(timer, state, "movie"), "30 s left")
    tkEqual(sleepState(timer, 1000# + 15 * 60000# - 1500#, 5000#, "a").secondsLeft, 2)
    tkTrue(sleepState(timer, 1000# + 15 * 60000#, 5000#, "a").fire)
end sub

sub test_sleep_keep_watching_adds_the_same_time()
    timer = sleepNew(30, 0#)
    sleepExtend(timer, "a")
    tkEqual(sleepState(timer, 30 * 60000#, -1#, "a").minutesLeft, 30, "the next 30 minutes")
    tkFalse(sleepEndsWith(timer, "a"), "minutes never end with a title")
end sub

sub test_sleep_at_the_end_of_what_plays()
    timer = sleepNew(-1, 0#)
    tkEqual(timer.deadlineMs, 0#)
    state = sleepState(timer, 0#, 125000#, "ep1")
    tkFalse(state.fire)
    tkEqual(state.minutesLeft, 3)
    tkEqual(sleepLabel(timer, state, "episode"), "End of this episode")
    state = sleepState(timer, 0#, 9000#, "ep1")
    tkEqual(state.secondsLeft, 9)
    tkEqual(sleepLabel(timer, state, "episode"), "9 s left")
    tkEqual(sleepState(timer, 0#, -1#, "ep1").minutesLeft, -1, "not known yet")
    tkTrue(sleepEndsWith(timer, "ep1"))

    ' Keep watching: this one plays to its end and on, the next one's end stops.
    sleepExtend(timer, "ep1")
    tkFalse(sleepEndsWith(timer, "ep1"))
    tkEqual(sleepState(timer, 0#, 9000#, "ep1").secondsLeft, -1, "no notice for the title spared")
    tkEqual(sleepState(timer, 0#, 9000#, "ep2").secondsLeft, 9, "the next one's end")
    tkEqual(timer.spared, "", "forgotten once the next one plays")
    tkTrue(sleepEndsWith(timer, "ep2"))
end sub

sub test_sleep_off_says_nothing()
    timer = sleepNew(0, 0#)
    state = sleepState(timer, 99999#, 1000#, "a")
    tkFalse(state.fire)
    tkEqual(state.secondsLeft, -1)
    tkEqual(state.minutesLeft, -1)
    tkEqual(sleepLabel(timer, state, "episode"), "Off")
    tkFalse(sleepEndsWith(timer, "a"))
end sub
