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

sub test_sleep_button_steps_through_the_choices()
    steps = []
    minutes = 0
    for i = 1 to 7
        minutes = sleepNextMinutes(minutes)
        steps.Push(minutes)
    end for
    tkEqual(steps, [15, 30, 45, 60, 90, -1, 0], "Off after the end of the track")
    tkEqual(sleepNextMinutes(20), 0, "a time it doesn't offer: Off")
end sub

sub test_sleep_button_says_what_is_left()
    timer = sleepNew(0, 0#)
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 0#, 1000#, "a")), "Sleep timer")
    timer = sleepNew(15, 0#)
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 0#, 1000#, "a")), "Sleep timer: 15 min left")
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 14.5 * 60000#, 1000#, "a")), "Keep listening", "its last 30 s")
    timer = sleepNew(-1, 0#)
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 0#, 125000#, "a")), "Sleep timer: End of this track")
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 0#, -1#, "a")), "Sleep timer: End of this track", "a track not started")
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 0#, 9000#, "a")), "Keep listening")
end sub

sub test_sleep_words_in_french_and_spanish()
    i18nUse("fr")
    tkEqual(sleepChoices("episode")[0].label, "Désactivé")
    tkEqual(sleepChoices("movie").Peek().label, "Fin de ce film")
    timer = sleepNew(15, 0#)
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 0#, 1000#, "a")), "Minuteur" + Chr(160) + ": Il reste 15 min")
    tkEqual(sleepButtonLabel(timer, sleepState(timer, 14.5 * 60000#, 1000#, "a")), "Continuer à écouter")
    i18nUse("es")
    tkEqual(sleepLabel(timer, sleepState(timer, 14 * 60000#, 1000#, "a"), "movie"), "Queda 1 min", "one")
    tkEqual(sleepLabel(timer, sleepState(timer, 0#, 1000#, "a"), "movie"), "Quedan 15 min")
    tkEqual(sleepLabel(timer, sleepState(timer, 14.5 * 60000#, 1000#, "a"), "movie"), "Quedan 30 s")
    tkEqual(sleepButtonLabel(sleepNew(0, 0#), sleepState(sleepNew(0, 0#), 0#, 1000#, "a")), "Temporizador")
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
