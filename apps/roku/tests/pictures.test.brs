' A file with text, PGS and VobSub subtitles, PGS picked.
function picTestFile() as object
    return {
        id: 7,
        selected_subtitle: 4,
        streams: [
            { index: 0, kind: "video", codec: "hevc" },
            { index: 3, kind: "subtitle", codec: "subrip", text_based: true },
            { index: 4, kind: "subtitle", codec: "hdmv_pgs_subtitle", text_based: false },
            { index: 5, kind: "subtitle", codec: "dvd_subtitle", text_based: false },
            { index: 1004, kind: "subtitle", codec: "hdmv_pgs_subtitle", text_based: false, external: true }
        ]
    }
end function

' A list as the server sends it: two cues in a 1920×1080 plane.
function picTestList(complete as boolean) as object
    return {
        complete: complete,
        width: 1920,
        height: 1080,
        cues: [
            { start_ms: 1000, end_ms: 3000, x: 400, y: 900, width: 1120, height: 120, image: "subtitles-4-1000.png" },
            { start_ms: 5000, end_ms: 7500, x: 300, y: 860, width: 1320, height: 160, image: "subtitles-4-5000.png" }
        ]
    }
end function

sub test_pictures_drawn_are_pgs_the_person_picked()
    file = picTestFile()
    tkEqual(picTrack({ burn_subtitles: false }, file, false), 4)
    tkEqual(picTrack({ burn_subtitles: true, subtitle_index: 4 }, file, false), -1, "burned in by the server")
    tkEqual(picTrack({ burn_subtitles: false }, file, true), -1, "the server doesn't make pictures")
    file.selected_subtitle = 1004
    tkEqual(picTrack({}, file, false), 1004, "a .sup next to the media")
    file.selected_subtitle = 5
    tkEqual(picTrack({}, file, false), -1, "VobSub is burned in")
    file.selected_subtitle = 3
    tkEqual(picTrack({}, file, false), -1, "text: the Video node shows it")
    file.selected_subtitle = invalid
    tkEqual(picTrack({}, file, false), -1, "none picked")
    tkEqual(picTrack(invalid, file, false), -1)
    tkEqual(picTrack({}, invalid, false), -1)
end sub

sub test_pictures_are_beside_a_keyed_sessions_media()
    tkEqual(picFolder("/api/media/pley_mk_Ab3-_x/file.mkv", "http://nas:7539"), "/api/media/pley_mk_Ab3-_x/")
    tkEqual(picFolder("http://nas:7539/api/media/pley_mk_Ab3/master.m3u8", "http://nas:7539"), "/api/media/pley_mk_Ab3/", "after the server")
    tkEqual(picFolder("/api/media/pley_mk_Ab3/file.mkv?v=2", ""), "/api/media/pley_mk_Ab3/", "with a query")
    tkEqual(picFolder("/api/files/7/stream", "http://nas:7539"), "", "no key")
    tkEqual(picFolder("http://elsewhere/api/media/pley_mk_Ab3/file.mkv", "http://nas:7539"), "", "another server's")
    tkEqual(picFolder("/api/media/pley_mk_Ab3/../file.mkv", ""), "")
    tkEqual(picFolder(invalid, ""), "")
    tkEqual(picListPath("/api/media/pley_mk_Ab3/", 4, 754500), "/api/media/pley_mk_Ab3/subtitles-4.json?at=754500")
    tkEqual(picListPath("/api/media/pley_mk_Ab3/", 1004, -20), "/api/media/pley_mk_Ab3/subtitles-1004.json?at=0")
end sub

sub test_pictures_lists_are_read_and_checked()
    read = picRead(picTestList(false))
    tkEqual(read.complete, false)
    tkEqual([read.width, read.height], [1920, 1080])
    tkEqual(read.cues[0], { startMs: 1000, endMs: 3000, x: 400, y: 900, width: 1120, height: 120, image: "subtitles-4-1000.png" })
    tkEqual(read.cues.Count(), 2)
    tkInvalid(picRead({ width: 0, height: 1080, cues: [] }), "no plane")
    tkInvalid(picRead({ width: 1920, height: 1080 }), "no cues")
    tkInvalid(picRead("not a list"))
    tkEqual(picRead({ complete: true, width: 1920, height: 1080, cues: [] }).cues, [], "a track without a picture")
end sub

sub test_pictures_lists_leave_out_what_isnt_a_picture_here()
    list = picTestList(true)
    list.cues = [
        { start_ms: 1000, end_ms: 3000, x: 0, y: 0, width: 10, height: 10, image: "../secret.png" },
        { start_ms: 1000, end_ms: 3000, x: 0, y: 0, width: 10, height: 10, image: "http://elsewhere/a.png" },
        { start_ms: 1000, end_ms: 3000, x: 0, y: 0, width: 10, height: 10, image: "subtitles-4-1000.svg" },
        { start_ms: 1000, end_ms: 3000, x: 1900, y: 0, width: 40, height: 10, image: "wide.png" },
        { start_ms: 3000, end_ms: 3000, x: 0, y: 0, width: 10, height: 10, image: "no-time.png" },
        { start_ms: 4000, end_ms: 6000, x: 0, y: 0, width: 10, height: 10, image: "kept.png" },
        { start_ms: 5000, end_ms: 8000, x: 0, y: 0, width: 10, height: 10, image: "overlaps.png" },
        { start_ms: 6000, end_ms: 9000, x: 0, y: 0, width: 10, height: 10, image: "next.png" },
        "not a cue"
    ]
    names = []
    for each cue in picRead(list).cues
        names.Push(cue.image)
    end for
    tkEqual(names, ["kept.png", "next.png"])
end sub

sub test_pictures_answers_add_to_what_is_known()
    known = picRead(picTestList(false))
    later = picRead({ complete: false, width: 1920, height: 1080, cues: [
        { start_ms: 5000, end_ms: 7000, x: 300, y: 860, width: 1320, height: 160, image: "subtitles-4-5000.png" },
        { start_ms: 9000, end_ms: 9500, x: 300, y: 860, width: 1320, height: 160, image: "subtitles-4-9000.png" }
    ] })
    merged = picMerge(known, later)
    starts = []
    for each cue in merged.cues
        starts.Push(cue.startMs)
    end for
    tkEqual(starts, [1000, 5000, 9000], "the earlier cue kept, the newer answer's for its time")
    tkEqual(merged.cues[1].endMs, 7000)
    tkFalse(merged.complete)
    empty = picMerge(known, picRead({ complete: false, width: 1920, height: 1080, cues: [] }))
    tkEqual(empty.cues.Count(), 2, "nothing read there yet: what is known stays")
    whole = picRead({ complete: true, width: 1920, height: 1080, cues: [] })
    tkEqual(picMerge(known, whole), whole, "a whole track in place of the rest")
    tkEqual(picMerge(invalid, later), later)
end sub

sub test_pictures_show_from_their_start_until_their_end()
    pictures = picRead(picTestList(true))
    tkEqual(picPlan(pictures, 500, 0), { show: -1, preload: 0 }, "before the first: it loads")
    tkEqual(picPlan(pictures, 1000, 0), { show: 0, preload: 1 })
    tkEqual(picPlan(pictures, 2999, 0), { show: 0, preload: 1 })
    tkEqual(picPlan(pictures, 3000, 0), { show: -1, preload: 1 }, "gone at its end")
    tkEqual(picPlan(pictures, 7500, 0), { show: -1, preload: -1 })
    tkEqual(picPlan(invalid, 1000, 0), { show: -1, preload: -1 })
end sub

sub test_pictures_move_with_the_subtitle_sync()
    pictures = picRead(picTestList(true))
    tkEqual(picPlan(pictures, 1000, 500).show, -1, "half a second later")
    tkEqual(picPlan(pictures, 1500, 500).show, 0)
    tkEqual(picPlan(pictures, 3400, 500).show, 0)
    tkEqual(picPlan(pictures, 500, -500).show, 0, "earlier")
end sub

sub test_pictures_are_placed_on_the_screen_as_the_video_is()
    pictures = picRead(picTestList(true))
    cue = pictures.cues[0]
    tkEqual(picBox(cue, pictures, 1920, 1080), { x: 400, y: 900, width: 1120, height: 120 })
    tkEqual(picBox(cue, pictures, 1280, 720), { x: 267, y: 600, width: 747, height: 80 })
    ' A 4:3 plane, centred between bars.
    square = { width: 720, height: 540 }
    tkEqual(picBox({ x: 0, y: 500, width: 720, height: 40 }, square, 1920, 1080), { x: 240, y: 1000, width: 1440, height: 80 })
end sub

sub test_pictures_answers_say_whether_to_burn_them_in()
    tkEqual(picAnswerKind({ ok: true, status: 200, data: picRead(picTestList(false)), error: "" }), "ok")
    tkEqual(picAnswerKind({ ok: true, status: 200, data: invalid, error: "" }), "unavailable", "not a list")
    tkEqual(picAnswerKind({ ok: false, status: 404, data: invalid, error: "not_found" }), "unavailable", "an older server")
    tkEqual(picAnswerKind({ ok: false, status: 503, data: invalid, error: "transcoder_outdated" }), "unavailable", "a transcoder too old")
    tkEqual(picAnswerKind({ ok: false, status: 503, data: invalid, error: "network" }), "again", "not Pley's answer")
    tkEqual(picAnswerKind({ ok: false, status: -28, data: invalid, error: "network" }), "again")
    tkEqual(picAnswerKind({ ok: false, status: 500, data: invalid, error: "internal" }), "again")
    tkEqual(picAnswerKind({ ok: false, status: 410, data: invalid, error: "session_stopped" }), "stopped")
end sub
