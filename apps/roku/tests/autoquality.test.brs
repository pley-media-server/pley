sub test_aq_the_menu_in_its_order()
    ids = []
    labels = []
    for each quality in aqMenu()
        ids.Push(quality.id)
        labels.Push(quality.label)
    end for
    tkEqual(ids, ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"], "the two 4K qualities first")
    tkEqual(labels, ["Original", "Automatic", "30 Mbps 4K", "15 Mbps 4K", "20 Mbps 1080p", "12 Mbps 1080p", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p"])
    tkEqual(aqKbps("15m"), 15000)
    tkEqual(aqLines("15m"), 2160)
    tkEqual(aqKbps("auto"), 0)
    tkEqual(aqLabel("nonsense"), "Original")
end sub

sub test_aq_automatic_unless_a_quality_was_chosen()
    section = tkRegistrySection()
    tkEqual(aqChoice(section), "auto", "nothing chosen")
    aqChoose(section, "original")
    tkEqual(aqChoice(section), "original", "Original chosen is kept")
    aqChoose(section, "8m")
    tkEqual(aqChoice(section), "8m")
    tkEqual(section.flushes, 2)
    section.Write("quality", "junk")
    tkEqual(aqChoice(section), "auto")
end sub

sub test_aq_a_file_is_offered_the_qualities_that_make_it_smaller()
    tkEqual(aqOffered(10000, 1080), ["8m", "4m", "2m"], "at most 80 % of its bitrate")
    tkEqual(aqOffered(18000, 2160), ["20m", "12m", "8m", "4m", "2m"], "15 Mbps 4K is no lighter than 18")
    tkEqual(aqOffered(20000, 1080), ["15m", "12m", "8m", "4m", "2m"], "15 Mbps 4K as 15 Mbps 1080p, above 12")
    tkEqual(aqOffered(40000, 2160), ["30m", "15m", "20m", "12m", "8m", "4m", "2m"], "a 4K file: the menu's order")
    tkEqual(aqOffered(0, 0), ["30m", "15m", "20m", "12m", "8m", "4m", "2m"], "unknown: every one")
    tkEqual(aqOffered(1500, 480), [])
end sub

sub test_aq_the_players_menu_names_qualities_by_the_files_lines()
    labels = function(menu as object) as object
        out = []
        for each quality in menu
            out.Push(quality.label)
        end for
        return out
    end function
    tkEqual(labels(aqMenuFor({ bitrate: 12000000, width: 1280, height: 720 })), ["Original", "Automatic", "8 Mbps 720p", "4 Mbps 720p", "2 Mbps 480p"])
    tkEqual(labels(aqMenuFor({ bitrate: 10000000, width: 1920, height: 800 })), ["Original", "Automatic", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p"], "a wide film is 1080p")
    tkEqual(labels(aqMenuFor(invalid)).Count(), 9, "an unknown file: the whole menu")
    tkEqual(aqLadder(40000, 2160, ""), ["30m", "15m", "12m", "8m", "4m", "2m"], "each lower in bitrate than the one above")
    tkEqual(aqLadder(40000, 2160, "15m"), ["15m", "12m", "8m", "4m", "2m"], "never above the server's quality")
end sub

sub test_aq_the_menu_in_french_and_spanish()
    i18nUse("fr")
    menu = aqMenuFor({ bitrate: 12000000, width: 1280, height: 720 })
    tkEqual([menu[0].label, menu[1].label, menu[2].label], ["Original", "Automatique", "8 Mbit/s 720p"])
    tkEqual(aqLabel("30m"), "30 Mbit/s 4K")
    tkEqual(aqBitrateLabel(4500, 1080), "4,5 Mbit/s 1080p")
    tkEqual(aqBitrateLabel(3000, 0), "3 Mbit/s")
    tkEqual(aqHint("auto"), "S’adapte aux conditions.")
    i18nUse("es")
    tkEqual(aqLabel("auto"), "Automática")
    tkEqual(aqBitrateLabel(4500, 2160), "4,5 Mbps 4K")
    tkEqual(aqMenu()[2], { id: "30m", label: "30 Mbps 4K", kbps: 30000, lines: 2160 }, "Settings' menu")
end sub

' What the menu has picked for a file, as the reports and the user's
' remotes say it: the choice where the file offers it, else Original,
' which it plays.
sub test_aq_the_menus_pick_for_a_file()
    hd = { bitrate: 12000000, width: 1280, height: 720 }
    tkEqual(aqMenuIds(hd), ["original", "auto", "8m", "4m", "2m"])
    tkEqual(aqPicked("auto", hd), "auto")
    tkEqual(aqPicked("original", hd), "original")
    tkEqual(aqPicked("4m", hd), "4m")
    tkEqual(aqPicked("30m", hd), "original", "a quality the file doesn't offer plays the original")
    tkEqual(aqPicked("30m", invalid), "30m", "an unknown file: the whole menu")
end sub

sub test_aq_how_each_choice_is_asked_for()
    big = { bitrate: 24000000, height: 2160 }
    small = { bitrate: 6000000, streams: [{ kind: "video", height: 1080 }] }
    tkEqual(aqAsk("original", "", 0, big), { automatic: false, quality: "original", lowered: false, mode: "auto", maxBitrate: 0, maxHeight: 0 })
    tkEqual(aqAsk("12m", "", 0, big), { automatic: false, quality: "12m", lowered: false, mode: "transcode", maxBitrate: 12000, maxHeight: 1080 })
    tkEqual(aqAsk("12m", "", 0, small).quality, "original", "a file already under it plays as it is")
    tkEqual(aqAsk("auto", "", 0, big), { automatic: true, quality: "original", lowered: false, mode: "auto", maxBitrate: 0, maxHeight: 0 })
    ' Under a ceiling remembered for a slow network.
    tkEqual(aqAsk("auto", "", 9450, big), { automatic: true, quality: "8m", lowered: true, mode: "transcode", maxBitrate: 8000, maxHeight: 1080 })
    tkEqual(aqAsk("auto", "", 9450, small).quality, "original", "a file under the ceiling plays as it is")
    ' Automatic's restarts.
    tkEqual(aqAsk("auto", "4m", 0, big), { automatic: true, quality: "4m", lowered: false, mode: "transcode", maxBitrate: 4000, maxHeight: 720 })
    tkEqual(aqAsk("auto", "original", 9450, big).mode, "auto")
end sub

sub test_aq_steps_down_by_what_the_network_carried()
    ' A 40 Mbps file, segments arriving at 20 Mbps: 15 with the margin.
    tkEqual(aqStepDown("original", 40000, 20000, 40000, 2160, ""), { quality: "15m", ceiling: 15000 })
    ' A file played as it is, no segments: half the bitrate, then the margin.
    tkEqual(aqStepDown("original", 20000, 0, 20000, 1080, ""), { quality: "4m", ceiling: 7500 })
    tkEqual(aqStepDown("original", 60000, 50000, 60000, 2160, "15m").quality, "15m", "never above the server's own quality")
    tkEqual(aqStepDown("8m", 8000, 100000, 20000, 1080, "").quality, "4m", "always a quality lower")
    tkEqual(aqStepDown("4m", 4000, 100, 20000, 1080, "").quality, "2m", "never below 2 Mbps")
    tkInvalid(aqStepDown("2m", 2000, 100, 20000, 1080, ""))
    tkEqual(aqStepDown("original", 0, 0, 0, 0, "").quality, "8m", "nothing known")
end sub

sub test_aq_steps_up_one_at_a_time_then_the_original()
    tkEqual(aqStepUp("4m", 25000, 2160, ""), { quality: "8m", kbps: 8000 })
    tkEqual(aqStepUp("20m", 25000, 2160, ""), { quality: "original", kbps: 25000 })
    tkEqual(aqStepUp("15m", 60000, 2160, "15m"), { quality: "original", kbps: 15000 }, "the original converted is the server's quality")
    tkInvalid(aqStepUp("original", 25000, 2160, ""))
    tkInvalid(aqStepUp("30m", 0, 0, ""), "an unknown file can't be measured against")
end sub

sub test_aq_remembers_each_server_for_30_minutes()
    section = tkRegistrySection()
    tkEqual(aqRemembered(section, "http://nas:7539", 1000000), 0)
    aqRemember(section, "http://nas:7539", 9450.5, 1000000)
    tkEqual(aqRemembered(section, "http://nas:7539", 1000000 + 29 * 60), 9450)
    tkEqual(aqRemembered(section, "http://other:7539", 1000000), 0, "another server's network")
    tkEqual(aqRemembered(section, "http://nas:7539", 1000000 + 31 * 60), 0)
    aqRemember(section, "http://nas:7539", 0, 1000060)
    tkEqual(aqRemembered(section, "http://nas:7539", 1000060), 0, "back at the original, forgotten")
end sub

' What a step of the watch's says: the quality a restart goes to, the
' bitrate a stream goes on at ("4000 kbps"), or "next 8m"; "" for none.
function aqSaid(made as dynamic) as string
    if made = invalid then return ""
    if made.kind = "bitrate" then return made.kbps.ToStr() + " kbps"
    if made.kind = "next" then return "next " + made.quality
    return made.quality
end function

' A Roku playing: a tick each second, a 1 MB segment every 2 s arriving at
' segmentKbps (none for 0). What the watch said: [{ at, said, made }].
function aqPlay(watch as object, fromMs as integer, seconds as integer, segmentKbps as float) as object
    said = []
    now = fromMs
    for i = 1 to seconds
        now = now + 1000
        if segmentKbps > 0 and i MOD 2 = 0 then aqSegment(watch, now, 1000000, 8000000 / segmentKbps)
        made = aqTick(watch, now)
        if made <> invalid then said.Push({ at: now, said: aqSaid(made), made: made })
    end for
    return { now: now, said: said }
end function

' A wait of seconds from atMs (the Roku says it is an underrun), then
' playing again: what the watch said meanwhile ("" for nothing).
function aqWait(watch as object, atMs as integer, seconds as integer) as string
    said = aqSaid(aqState(watch, atMs, "buffering", true))
    for i = 1 to seconds
        made = aqTick(watch, atMs + i * 1000)
        if made <> invalid then said = aqSaid(made)
    end for
    made = aqState(watch, atMs + seconds * 1000, "playing", invalid)
    if made <> invalid then said = aqSaid(made)
    return said
end function

' Two waits of half a second, 20 s apart, from atMs: what the watch said.
function aqShortWaits(watch as object, atMs as integer) as string
    aqState(watch, atMs, "buffering", invalid)
    aqState(watch, atMs + 500, "playing", invalid)
    aqPlay(watch, atMs + 500, 20, 0)
    aqState(watch, atMs + 21000, "buffering", invalid)
    return aqSaid(aqState(watch, atMs + 21800, "playing", invalid))
end function

' A conversion whose bitrate changes as it is sent (live_bitrate).
function aqLive(quality as string, playing as float, fileKbps as float, lines as integer) as object
    return aqWatch(0, quality, playing, fileKbps, lines, "", false, true, lines)
end function

sub test_aq_a_long_wait_moves_the_original_to_a_conversion()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    tkInvalid(aqState(watch, 1000, "playing", invalid))
    played = aqPlay(watch, 1000, 30, 12000)
    tkInvalid(aqState(watch, played.now, "buffering", true))
    tkInvalid(aqTick(watch, played.now + 1000))
    ' 12 Mbps carried, 9 with the margin.
    tkEqual(aqTick(watch, played.now + 2000), { kind: "restart", quality: "8m", kbps: 0, down: true, memory: 9000 })
    tkEqual(watch.ceiling, 9000)
    tkInvalid(aqTick(watch, played.now + 3000), "one wait, one step")
end sub

sub test_aq_two_short_waits_dont_move_the_original()
    ' The picture plays on: another stream would blank the screen.
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 1000, "playing", invalid)
    aqPlay(watch, 1000, 30, 0)
    tkEqual(aqShortWaits(watch, 31000), "")
    tkEqual(watch.issueAt, 52800, "still an issue: the calm starts again")
    tkEqual(aqPlay(watch, 52800, 60, 0).said, [])
end sub

sub test_aq_waits_that_arent_the_network()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    ' The start.
    aqState(watch, 0, "buffering", invalid)
    tkEqual(aqPlay(watch, 0, 5, 0).said, [])
    aqState(watch, 5000, "playing", invalid)
    aqPlay(watch, 5000, 30, 0)
    ' A seek pley made.
    aqSeeked(watch)
    aqState(watch, 35000, "buffering", invalid)
    tkEqual(aqPlay(watch, 35000, 5, 0).said, [])
    aqState(watch, 40000, "playing", invalid)
    aqPlay(watch, 40000, 30, 0)
    ' The Roku says this wait isn't an underrun (a seek in its own controls).
    aqState(watch, 70000, "buffering", false)
    tkEqual(aqPlay(watch, 70000, 5, 0).said, [])
    ' Paused, then buffering on the way back.
    aqState(watch, 75000, "playing", invalid)
    aqPlay(watch, 75000, 10, 0)
    aqState(watch, 85000, "paused", invalid)
    aqState(watch, 95000, "buffering", invalid)
    tkEqual(aqPlay(watch, 95000, 5, 0).said, [])
end sub

sub test_aq_not_again_within_20_seconds()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 0, "playing", invalid)
    aqPlay(watch, 0, 30, 12000)
    aqState(watch, 30000, "buffering", true)
    tkEqual(aqSaid(aqTick(watch, 32000)), "8m")
    ' A server whose conversions keep their bitrate: another stream again.
    aqRestarted(watch, 34000, "8m", 8000)
    aqState(watch, 35000, "playing", invalid)
    aqPlay(watch, 35000, 5, 0)
    aqState(watch, 40000, "buffering", true)
    tkInvalid(aqTick(watch, 43000), "within 20 s of the change")
    aqState(watch, 44000, "playing", invalid)
    aqPlay(watch, 44000, 20, 100000)
    aqState(watch, 64000, "buffering", true)
    tkEqual(aqSaid(aqTick(watch, 66000)), "4m", "a quality lower at least")
end sub

sub test_aq_a_file_played_as_it_is_stays_down()
    watch = aqWatch(0, "8m", 8000, 20000, 1080, "", true)
    aqState(watch, 0, "playing", invalid)
    tkEqual(aqPlay(watch, 0, 1200, 0).said, [], "no segments, no measure")
end sub

sub test_aq_a_seek_in_the_rokus_own_controls_isnt_a_wait()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 0, "playing", invalid)
    for second = 1 to 30
        aqPosition(watch, second * 1000, second * 1000)
    end for
    ' Its wait came first, the jump ahead after.
    aqState(watch, 31000, "buffering", invalid)
    aqPosition(watch, 32000, 600000)
    tkInvalid(aqTick(watch, 34000), "the wait was the seek's")
    aqState(watch, 34000, "playing", invalid)
    ' Playing on, a real wait counts again: on a network the segments show at 8 Mbps.
    for second = 35 to 40
        aqPosition(watch, second * 1000, 600000 + (second - 34) * 1000)
        if second MOD 2 = 0 then aqSegment(watch, second * 1000, 1000000, 1000)
    end for
    aqState(watch, 40500, "buffering", invalid)
    tkEqual(aqSaid(aqTick(watch, 42600)), "4m")
end sub

sub test_aq_the_original_plays_on_through_a_wait_on_a_network_that_carries_it()
    ' Segments at 32 Mbps, over 1.5 times the file's 20: the server is why it waits.
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 1000, "playing", invalid)
    played = aqPlay(watch, 1000, 30, 32000)
    tkEqual(aqWait(watch, played.now, 2), "")
    played = aqPlay(watch, played.now + 2000, 20, 32000)
    tkEqual(aqWait(watch, played.now, 8), "")
    tkEqual(watch.issueAt, 0, "neither wait an issue, nor the two of them short waits")
    tkEqual(watch.quality, "original")
end sub

sub test_aq_a_slow_segment_as_a_wait_ends_shows_the_network_to_blame()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 1000, "playing", invalid)
    aqSegment(watch, 2000, 1000000, 200)
    played = aqPlay(watch, 2000, 25, 0)
    aqState(watch, played.now, "buffering", true)
    tkInvalid(aqTick(watch, played.now + 2000), "one segment at 40 Mbps: nothing measured yet")
    ' Two more, at 8 Mbps, arrive as it waits.
    aqSegment(watch, played.now + 2500, 1000000, 1000)
    aqSegment(watch, played.now + 2900, 1000000, 1000)
    tkEqual(aqSaid(aqState(watch, played.now + 3000, "playing", invalid)), "4m", "8 Mbps arriving, 6 with the margin")
end sub

sub test_aq_a_direct_play_moves_at_the_second_wait_within_5_minutes()
    ' No segments: nothing measures the network.
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 1000, "playing", invalid)
    played = aqPlay(watch, 1000, 30, 0)
    tkEqual(aqWait(watch, played.now, 4), "")
    played = aqPlay(watch, played.now + 4000, 60, 0)
    tkEqual(aqWait(watch, played.now, 3), "4m", "half the file with the margin: 7.5")
    apart = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(apart, 1000, "playing", invalid)
    played = aqPlay(apart, 1000, 30, 0)
    tkEqual(aqWait(apart, played.now, 3), "")
    played = aqPlay(apart, played.now + 3000, 300, 0)
    tkEqual(aqWait(apart, played.now, 3), "", "5 minutes apart")
end sub

sub test_aq_the_original_converted_moves_on_any_long_wait()
    ' Converted at the server's own quality: a lower one is less for it to make.
    watch = aqWatch(0, "original", 15000, 40000, 2160, "15m", false, false, 2160, true)
    aqState(watch, 1000, "playing", invalid)
    played = aqPlay(watch, 1000, 30, 0)
    tkEqual(aqWait(watch, played.now, 2), "4m", "half its 15 Mbps with the margin: 5.6")
end sub

' --- A conversion whose bitrate changes as it is sent ---

sub test_aq_a_long_wait_lowers_the_bitrate_and_keeps_the_size()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 6000)
    aqState(watch, played.now, "buffering", true)
    ' 6 Mbps carried, 4.5 with the margin, 4 in whole Mbps.
    tkEqual(aqTick(watch, played.now + 2000), { kind: "bitrate", quality: "8m", kbps: 4000, down: true, memory: 4500 })
    tkEqual([watch.quality, watch.lines, watch.playingKbps], ["8m", 1080, 4000], "the same stream, the same size")
    tkEqual(aqBitrateLabel(watch.playingKbps, watch.lines), "4 Mbps 1080p")
    tkTrue(aqStepped(watch))
    tkTrue(aqLowered(watch))
end sub

sub test_aq_never_below_its_sizes_floor()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 6000)
    tkEqual(aqWait(watch, played.now, 2), "4000 kbps")
    played = aqPlay(watch, played.now + 2000, 20, 6000)
    tkEqual(aqWait(watch, played.now, 2), "3000 kbps", "4 × 0.75 is 3: 3 Mbps for 1080p")
    played = aqPlay(watch, played.now + 2000, 30, 6000)
    tkEqual(aqWait(watch, played.now, 5), "", "at the floor the network waits")
    ' 4K goes no lower than 8 Mbps.
    uhd = aqLive("15m", 15000, 40000, 2160)
    aqState(uhd, 0, "playing", invalid)
    played = aqPlay(uhd, 0, 30, 0)
    tkEqual(aqWait(uhd, played.now, 2), "8000 kbps", "half, then the margin: 5.6, under 8")
    tkEqual(aqBitrateLabel(uhd.playingKbps, uhd.lines), "8 Mbps 4K")
end sub

sub test_aq_two_short_waits_lower_the_bitrate()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 1000, "playing", invalid)
    aqPlay(watch, 1000, 30, 0)
    tkEqual(aqShortWaits(watch, 31000), "3000 kbps")
end sub

sub test_aq_back_up_one_step_at_a_time_to_its_own()
    watch = aqLive("20m", 20000, 25000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 8000)
    tkEqual(aqWait(watch, played.now, 2), "6000 kbps")
    played = aqPlay(watch, played.now + 2000, 299, 34000)
    tkEqual(played.said, [], "not within 5 minutes of a step down")
    played = aqPlay(watch, played.now, 1, 34000)
    tkEqual(played.said[0].made, { kind: "bitrate", quality: "20m", kbps: 8000, down: false, memory: 8000 }, "the menu's next bitrate up")
    played = aqPlay(watch, played.now, 180, 34000)
    tkEqual(played.said[0].said, "12000 kbps", "3 minutes on")
    played = aqPlay(watch, played.now, 180, 34000)
    tkEqual(played.said[0].said, "15000 kbps")
    played = aqPlay(watch, played.now, 180, 34000)
    tkEqual(played.said[0].made, { kind: "bitrate", quality: "20m", kbps: 20000, down: false, memory: 20000 }, "its own: the quality's")
    tkFalse(aqStepped(watch))
    tkEqual(aqPlay(watch, played.now, 600, 34000).said, [], "never above its own")
end sub

sub test_aq_back_at_the_original_converted_forgets_the_ceiling()
    watch = aqWatch(0, "original", 15000, 40000, 2160, "15m", false, true, 2160, true)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 15000)
    tkEqual(aqWait(watch, played.now, 2), "11000 kbps")
    played = aqPlay(watch, played.now + 2000, 300, 30000)
    tkEqual(played.said[0].made, { kind: "bitrate", quality: "original", kbps: 12000, down: false, memory: 12000 })
    played = aqPlay(watch, played.now, 180, 30000)
    tkEqual(played.said[0].made, { kind: "bitrate", quality: "original", kbps: 15000, down: false, memory: 0 }, "its own: no limit")
    tkFalse(aqLowered(watch))
end sub

sub test_aq_the_server_says_what_it_goes_on_at()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 0)
    tkEqual(aqWait(watch, played.now, 2), "3000 kbps")
    aqAnswered(watch, 3000, 3500)
    tkEqual(watch.playingKbps, 3500)
end sub

sub test_aq_refused_it_moves_as_a_stream_that_cant_change()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 0)
    tkEqual(aqWait(watch, played.now, 2), "3000 kbps")
    ' A transcoder from before: 400.
    aqAnswered(watch, 3000, 0)
    tkEqual([watch.live, watch.playingKbps], [false, 8000], "as it was")
    played = aqPlay(watch, played.now + 2000, 30, 0)
    tkEqual(aqShortWaits(watch, played.now), "", "the picture plays on")
    tkEqual(aqWait(watch, played.now + 30000, 2), "2m", "another stream once it stopped: 8 Mbps halved, with the margin")
end sub

' --- It settles ---

sub test_aq_a_bitrate_that_didnt_hold_isnt_tried_again()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 6000)
    tkEqual(aqWait(watch, played.now, 2), "4000 kbps")
    played = aqPlay(watch, played.now + 2000, 300, 13000)
    tkEqual(played.said[0].said, "8000 kbps")
    ' Two minutes on, 8 Mbps doesn't hold after all.
    played = aqPlay(watch, played.now, 120, 6000)
    tkEqual(aqWait(watch, played.now, 2), "4000 kbps")
    tkEqual(watch.calmMs, 360000)
    ' However fast the network says it is, it stays where it held.
    tkEqual(aqPlay(watch, played.now + 2000, 3600, 80000).said, [])
end sub

sub test_aq_a_step_down_more_than_10_minutes_after_leaves_it_open()
    watch = aqLive("8m", 8000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 6000)
    aqWait(watch, played.now, 2)
    played = aqPlay(watch, played.now + 2000, 300, 13000)
    played = aqPlay(watch, played.now, 605, 6000)
    tkEqual(aqWait(watch, played.now, 2), "4000 kbps")
    tkEqual(watch.calmMs, 180000)
    played = aqPlay(watch, played.now + 2000, 300, 13000)
    tkEqual(played.said[0].said, "8000 kbps")
end sub

sub test_aq_each_step_undone_doubles_the_calm()
    watch = aqLive("12m", 12000, 20000, 1080)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 7000)
    tkEqual(aqWait(watch, played.now, 2), "5000 kbps")
    played = aqPlay(watch, played.now + 2000, 480, 30000)
    tkEqual([played.said[0].said, played.said[1].said], ["8000 kbps", "12000 kbps"])
    ' 12 Mbps doesn't hold: back down, and it's off for the title.
    played = aqPlay(watch, played.now, 20, 11000)
    tkEqual(aqWait(watch, played.now, 2), "8000 kbps")
    ' Later 8 falters too, more than 10 minutes after it was reached: 8 stays open.
    played = aqPlay(watch, played.now + 2000, 620, 8000)
    tkEqual(aqWait(watch, played.now, 2), "6000 kbps")
    ' 5 minutes after the step down 3 calm minutes would have done; now it takes 6.
    played = aqPlay(watch, played.now + 2000, 300, 30000)
    tkEqual(played.said, [])
    played = aqPlay(watch, played.now, 60, 30000)
    tkEqual(played.said[0].said, "8000 kbps")
    tkEqual(aqPlay(watch, played.now, 1200, 30000).said, [], "12 Mbps not again")
end sub

sub test_aq_the_calm_doubles_up_to_12_minutes()
    watch = aqLive("8m", 8000, 20000, 1080)
    for i = 1 to 4
        watch.lastUpAt = 0
        watch.lastUpKey = "x"
        aqWentDown(watch, 1000)
    end for
    tkEqual(watch.calmMs, 720000)
end sub

' --- Where the next title starts ---

sub test_aq_where_the_next_title_starts_goes_up_one_quality_at_a_time()
    watch = aqWatch(0, "4m", 4000, 20000, 1080, "", true)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 299, 13000)
    tkEqual(played.said, [], "not within 5 minutes of a step down")
    played = aqPlay(watch, played.now, 1, 13000)
    tkEqual(played.said[0].made, { kind: "next", quality: "8m", kbps: 0, down: false, memory: 8000 }, "13 Mbps carries 1.6 times 8")
    tkEqual(watch.quality, "4m", "the stream plays on as it is")
    tkEqual(aqPlay(watch, played.now, 400, 13000).said, [], "12 Mbps needs 19.2")
    ' The network carries 40 Mbps from then on.
    played = aqPlay(watch, 700000, 600, 40000)
    said = []
    for each one in played.said
        said.Push(one.said)
    end for
    ' 15 Mbps makes the 20 Mbps 1080p file smaller; 20 Mbps 1080p doesn't.
    tkEqual(said, ["next 12m", "next 15m", "next original"])
    tkEqual(played.said[1].at - played.said[0].at, 180000, "every 3 minutes at most")
    tkEqual(played.said[2].made.memory, 0, "40 Mbps carries 1.6 times the file: forgotten")
    tkEqual(aqPlay(watch, played.now, 600, 40000).said, [])
end sub

sub test_aq_a_raise_a_step_down_undoes_isnt_made_again()
    watch = aqWatch(0, "4m", 4000, 20000, 1080, "", true)
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 300, 13000)
    tkEqual(played.said[0].said, "next 8m")
    played = aqPlay(watch, played.now, 60, 0)
    aqState(watch, played.now, "buffering", true)
    made = aqTick(watch, played.now + 2000)
    tkEqual([made.quality, made.memory], ["2m", 2000], "the lowest, above its ceiling")
    tkEqual(watch.calmMs, 360000)
    aqRestarted(watch, played.now + 3000, "2m", 2000)
    aqState(watch, played.now + 3000, "playing", invalid)
    played = aqPlay(watch, played.now + 3000, 359, 50000)
    tkEqual(played.said, [], "6 calm minutes now")
    played = aqPlay(watch, played.now, 1, 50000)
    tkEqual(played.said[0].said, "next 4m")
    tkEqual(aqPlay(watch, played.now, 2000, 50000).said, [], "8 Mbps didn't hold this title")
end sub

sub test_aq_a_stream_in_several_sizes_leaves_its_waits_to_the_roku()
    watch = aqWatch(0, "8m", 8000, 20000, 1080, "", true, true, 1080)
    watch.adaptive = true
    aqState(watch, 0, "playing", invalid)
    played = aqPlay(watch, 0, 30, 6000)
    tkEqual(aqWait(watch, played.now, 5), "", "the Video node moves to a smaller size")
    tkEqual(aqShortWaits(watch, played.now + 5000), "")
    ' Where the next title starts still goes up.
    played = aqPlay(watch, played.now + 30000, 300, 40000)
    tkEqual(played.said[0].said, "next 12m")
end sub

sub test_aq_the_original_playing_has_nothing_to_raise()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    aqState(watch, 0, "playing", invalid)
    tkEqual(aqPlay(watch, 0, 1200, 100000).said, [])
end sub

' --- Frames dropped (for a Roku that would say) ---

sub test_aq_dropped_frames_move_it_in_the_first_minute_only()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    tkEqual(aqDropped(watch, 30000), { kind: "restart", quality: "converted", kbps: 0, down: false, memory: -1 }, "the server's own quality")
    tkInvalid(aqDropped(watch, 40000), "not within 20 s of a change")
    tkEqual(aqDropped(aqLive("8m", 8000, 20000, 1080), 25000), { kind: "restart", quality: "4m", kbps: 0, down: true, memory: -1 }, "a quality lower")
    tkInvalid(aqDropped(aqLive("8m", 8000, 20000, 1080), 10000), "not within 20 s of the start")
end sub

sub test_aq_later_dropped_frames_lower_where_the_next_title_starts()
    watch = aqWatch(0, "original", 20000, 20000, 1080, "", false)
    tkEqual(aqDropped(watch, 61000), { kind: "next", quality: "15m", kbps: 0, down: false, memory: 15000 })
    tkEqual(watch.quality, "original", "it plays on")
    tkInvalid(aqDropped(watch, 90000), "said once")
    tkEqual(aqSaid(aqDropped(aqLive("8m", 8000, 20000, 1080), 300000)), "next 4m")
end sub

sub test_aq_labels_floors_and_steps_of_a_bitrate()
    tkEqual(aqBitrateLabel(4500, 1080), "4.5 Mbps 1080p")
    tkEqual(aqBitrateLabel(8000, 2160), "8 Mbps 4K")
    tkEqual(aqBitrateLabel(3000, 0), "3 Mbps")
    floors = []
    for each lines in [2160, 1600, 1080, 800, 720, 480, 0]
        floors.Push(aqFloorKbps(lines))
    end for
    tkEqual(floors, [8000, 8000, 3000, 3000, 1500, 1000, 1000])
    tkEqual(aqStreamLines("8m", true, "", 2160), 1080)
    tkEqual(aqStreamLines("8m", true, "", 720), 720, "no taller than the file")
    tkEqual(aqStreamLines("original", true, "15m", 2160), 2160)
    tkEqual(aqStreamLines("original", false, "", 1080), 1080)
    tkEqual(aqRungsUp(4500, 20000), [8000, 12000, 15000, 20000])
    tkEqual(aqRungsUp(3000, 6000), [4000, 6000])
    tkEqual(aqFileWidth({ streams: [{ kind: "audio" }, { kind: "video", width: 1920 }] }), 1920)
end sub

sub test_aq_the_servers_word_on_the_network()
    tkEqual(aqNetworkCeiling({ local: false, ceiling_kbps: 8000, source: "playback", probe: false }), 8000)
    tkEqual(aqNetworkCeiling({ ceiling_kbps: invalid }), 0, "no limit")
    tkEqual(aqNetworkCeiling({ ceiling_kbps: true }), 0)
    tkEqual(aqNetworkCeiling(invalid), 0)
    tkEqual(aqWantsProbe({ local: false, probe: true }), true)
    tkEqual(aqWantsProbe({ local: true, probe: true }), false, "never on the server's own network")
    tkEqual(aqWantsProbe({ probe: 1 }), false)
    tkEqual(aqWantsProbe(invalid), false)
    ' Its ceiling where the remembered one would go: 9 Mbps starts at 8.
    asked = aqAsk("auto", "", aqNetworkCeiling({ ceiling_kbps: 9000 }), { bitrate: 20000000, height: 1080 })
    tkEqual([asked.quality, asked.lowered, asked.maxBitrate], ["8m", true, 8000])
end sub

sub test_aq_a_speed_test_fetched_whole()
    ' 8 MiB in 2 s, the whole request timed: never "complete".
    result = aqProbeResult(8388608, 2000)
    tkEqual(result, { probe_kbps: 33554, probe_complete: false })
    tkEqual(Int(aqProbeCeiling(result)), 23487, "70 % when the server can't keep it")
    tkEqual(aqProbeCeiling({ probe_kbps: 5000, probe_complete: true }), 0)
    tkEqual(aqProbeResult(200000, 3000), invalid, "under 256 KiB: nothing known")
    tkEqual(aqProbeResult(1000000, 0), invalid)
    tkEqual(aqProbeBytes(), 8388608)
    ' About 3.5 s in all: what asking took comes off the test, 1.5 s at least.
    tkEqual(aqProbeTimeoutMs(100), 2900)
    tkEqual(aqProbeTimeoutMs(1400), 1600)
    tkEqual(aqProbeTimeoutMs(2000), 1500)
end sub

sub test_aq_what_the_server_said_is_kept_ten_minutes()
    kept = {}
    tkEqual(aqNetworkKept(kept, "http://a", 0), invalid)
    answer = { ok: true, status: 200, data: { ceiling_kbps: 4000 }, error: "", message: "" }
    aqNetworkKeep(kept, "http://a", answer, 1000)
    tkEqual(aqNetworkKept(kept, "http://a", 600000).data.ceiling_kbps, 4000)
    tkEqual(aqNetworkKept(kept, "http://b", 600000), invalid, "each server its own")
    tkEqual(aqNetworkKept(kept, "http://a", 601001), invalid, "ten minutes")
    tkEqual(kept.Count(), 0)
    ' A server from before: its 404 is kept too, other failures aren't.
    aqNetworkKeep(kept, "http://a", { ok: false, status: 404, data: invalid, error: "not_found", message: "" }, 0)
    tkEqual(aqNetworkKept(kept, "http://a", 10).status, 404)
    aqNetworkKeep(kept, "http://b", { ok: false, status: -28, data: invalid, error: "network", message: "" }, 0)
    tkEqual(aqNetworkKept(kept, "http://b", 10), invalid)
end sub

sub test_aq_what_is_told_to_the_server()
    tkEqual(ParseJson(aqTellBody("r-1", { ceilingKbps: 8549.6 })), { client_id: "r-1", ceiling_kbps: 8550 })
    tkEqual(aqTellBody("r-1", { ceilingKbps: 0 }), "{""client_id"":""r-1"",""ceiling_kbps"":null}", "forgotten")
    tkEqual(ParseJson(aqTellBody("r-1", { probeKbps: 33554 })), { client_id: "r-1", probe_kbps: 33554, probe_complete: false })
end sub
