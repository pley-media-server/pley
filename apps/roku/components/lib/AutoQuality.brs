' The Quality menu and how Automatic chooses (docs/QUALITY.md), without
' SceneGraph: the player tells a watch what its Video node does, and the
' watch answers a step (aqStep): the title again at another quality, the
' stream's bitrate changed as it is sent, or where the next title starts.
'
' Roku's own HLS moves between the sizes of a conversion that comes in
' several (PlaybackInfo.adaptive), and Automatic leaves such a session to
' it. The Video node tells of waits (state "buffering",
' bufferingStatus.isUnderrun) and of each HLS segment it downloaded
' (downloadedSegment: its size and how long it took), which measures the
' network. A file played as it is has no segments: a step down from it
' halves, and nothing measures a way back up. Roku doesn't tell an app of
' the frames it drops; what it can't decode, the server converts (the
' player's fallback for an error).

' The qualities, in the menu's order: { id, kbps, lines }.
function aqQualities() as object
    return [
        { id: "original", kbps: 0, lines: 0 },
        { id: "auto", kbps: 0, lines: 0 },
        { id: "30m", kbps: 30000, lines: 2160 },
        { id: "15m", kbps: 15000, lines: 2160 },
        { id: "20m", kbps: 20000, lines: 1080 },
        { id: "12m", kbps: 12000, lines: 1080 },
        { id: "8m", kbps: 8000, lines: 1080 },
        { id: "4m", kbps: 4000, lines: 720 },
        { id: "2m", kbps: 2000, lines: 480 }
    ]
end function

' The Quality menu as Settings lists it: { id, label, kbps, lines }.
function aqMenu() as object
    menu = []
    for each quality in aqQualities()
        quality.label = aqLabel(quality.id)
        menu.Push(quality)
    end for
    return menu
end function

' Each quality's hint in Settings and the player's menu; the others need none.
function aqHint(id as string) as string
    if id = "original" then return t("player.quality.originalHint")
    if id = "auto" then return t("player.quality.automaticHint")
    return ""
end function

function aqFind(id as dynamic) as dynamic
    for each quality in aqQualities()
        if quality.id = httpString(id) then return quality
    end for
    return invalid
end function

' A quality's name: "Original", "Automatic", "8 Mbps 1080p" ("8 Mbit/s
' 1080p" in French); Original for an id not in the menu.
function aqLabel(id as dynamic) as string
    quality = aqFind(id)
    if quality = invalid or quality.id = "original" then return t("player.quality.original")
    if quality.id = "auto" then return t("player.quality.automatic")
    return aqPresetLabel(i18nNumber(quality.kbps \ 1000), quality.lines)
end function

' A bitrate (mbps, its words) and the lines it is named by: "8 Mbps
' 1080p", "15 Mbps 4K".
function aqPresetLabel(mbps as string, lines as integer) as string
    named = urlValue(lines) + "p"
    if lines >= 2160 then named = "4K"
    return t("player.quality.preset", { mbps: mbps, lines: named })
end function

' A quality's bitrate in kbps; 0 for Original and Automatic.
function aqKbps(id as dynamic) as integer
    quality = aqFind(id)
    if quality = invalid then return 0
    return quality.kbps
end function

function aqLines(id as dynamic) as integer
    quality = aqFind(id)
    if quality = invalid then return 0
    return quality.lines
end function

' The setting's quality: Automatic unless another was chosen.
function aqFromSetting(value as dynamic) as string
    if aqFind(value) <> invalid then return value
    return "auto"
end function

' The converted qualities worth offering for a file (docs/QUALITY.md):
' those with fewer lines than its fileLines, or at most 80 % of its
' bitrate, as a copy a little lighter plays no better than the file;
' every one, in the menu's order, when neither is known. Named by the
' file's lines when taller (aqMenuFor), they go by those lines, tallest
' first, then by bitrate: a 4K file keeps the menu's order.
function aqOffered(fileKbps as float, fileLines as integer) as object
    menu = ["30m", "15m", "20m", "12m", "8m", "4m", "2m"]
    if fileKbps <= 0 and fileLines <= 0 then return menu
    named = aqNamedLines(0, fileLines)
    copies = []
    for each id in menu
        fewer = fileLines > 0 and aqLines(id) < fileLines
        lighter = fileKbps > 0 and aqKbps(id) <= fileKbps * 0.8
        if fewer or lighter
            shown = aqLines(id)
            if named > 0 and named < shown then shown = named
            ' In order: after every copy taller, or as tall and of more bitrate.
            at = copies.Count()
            for i = copies.Count() - 1 to 0 step -1
                if copies[i].lines < shown or (copies[i].lines = shown and copies[i].kbps < aqKbps(id)) then at = i
            end for
            copies.Push(invalid)
            for i = copies.Count() - 1 to at + 1 step -1
                copies[i] = copies[i - 1]
            end for
            copies[at] = { id: id, lines: shown, kbps: aqKbps(id) }
        end if
    end for
    ids = []
    for each copy in copies
        ids.Push(copy.id)
    end for
    return ids
end function

' The lines qualities are named by for a file width × height: 2160, 1080,
' 720 or 480, by its width too (a 1920 × 800 film is 1080p); 0 when
' neither is known.
function aqNamedLines(width as integer, height as integer) as integer
    if width <= 0 and height <= 0 then return 0
    if width >= 3800 or height >= 2100 then return 2160
    if width >= 1800 or height >= 1000 then return 1080
    if width >= 1200 or height >= 700 then return 720
    return 480
end function

' The player's Quality menu for a file: Original, Automatic and the
' qualities offered for it ([{ id, label }]), a quality taller than the
' file named by the file's own lines ("8 Mbps 720p" for a 720p file).
function aqMenuFor(file as dynamic) as object
    named = aqNamedLines(aqFileWidth(file), aqFileLines(file))
    menu = [{ id: "original", label: aqLabel("original") }, { id: "auto", label: aqLabel("auto") }]
    for each id in aqOffered(aqFileKbps(file), aqFileLines(file))
        lines = aqLines(id)
        if named > 0 and lines > named then lines = named
        menu.Push({ id: id, label: aqPresetLabel(i18nNumber(aqKbps(id) \ 1000), lines) })
    end for
    return menu
end function

' The ids of the player's Quality menu for a file (aqMenuFor), in its
' order: asked with each progress report, so without their words.
function aqMenuIds(file as dynamic) as object
    ids = ["original", "auto"]
    ids.Append(aqOffered(aqFileKbps(file), aqFileLines(file)))
    return ids
end function

' What the player's Quality menu has picked for a file, as it ticks it and
' as progress reports and the user's remotes say it: the choice (Settings'
' or the menu's), else Original, which a file that doesn't offer the
' choice plays.
function aqPicked(choice as string, file as dynamic) as string
    if aqHas(aqMenuIds(file), choice) then return choice
    return "original"
end function

' The qualities Automatic steps through below the original, highest
' first: those offered for the file, no higher than the server's own
' quality for it (auto_quality, "" for none) in bitrate and lines, each
' of less bitrate than the one above (a 4K file goes 30 Mbps 4K, 15 Mbps
' 4K, then 12 Mbps 1080p).
function aqLadder(fileKbps as float, fileLines as integer, cap as string) as object
    most = aqKbps(cap)
    tallest = aqLines(cap)
    ladder = []
    below = 0
    for each id in aqOffered(fileKbps, fileLines)
        kbps = aqKbps(id)
        if (most <= 0 or (kbps <= most and aqLines(id) <= tallest)) and (below = 0 or kbps < below)
            ladder.Push(id)
            below = kbps
        end if
    end for
    return ladder
end function

function aqHas(ids as object, id as string) as boolean
    for each found in ids
        if found = id then return true
    end for
    return false
end function

' Where Automatic starts under a remembered ceiling (kbps, 0 for none):
' the best quality at or under it, 2 Mbps 480p at least; "" for none.
function aqStartQuality(ceilingKbps as float) as string
    if ceilingKbps <= 0 then return ""
    for each id in ["30m", "20m", "15m", "12m", "8m", "4m", "2m"]
        if aqKbps(id) <= ceilingKbps then return id
    end for
    return "2m"
end function

' A file's bitrate in kbps and lines, from its details (0 when unknown).
function aqFileKbps(file as dynamic) as float
    if file = invalid then return 0
    return httpNumber(file.bitrate, 0) / 1000.0
end function

function aqFileWidth(file as dynamic) as integer
    if file = invalid then return 0
    width = Int(httpNumber(file.width, 0))
    if width > 0 then return width
    for each stream in httpArray(file.streams)
        if GetInterface(stream, "ifAssociativeArray") <> invalid and httpString(stream.kind) = "video"
            width = Int(httpNumber(stream.width, 0))
            if width > 0 then return width
        end if
    end for
    return 0
end function

function aqFileLines(file as dynamic) as integer
    if file = invalid then return 0
    lines = Int(httpNumber(file.height, 0))
    if lines > 0 then return lines
    for each stream in httpArray(file.streams)
        if GetInterface(stream, "ifAssociativeArray") <> invalid and httpString(stream.kind) = "video" then return Int(httpNumber(stream.height, 0))
    end for
    return 0
end function

' How a playback is asked for: { automatic, quality, lowered, mode,
' maxBitrate, maxHeight } (0 for no limit). choice is the setting's
' quality; again Automatic's quality when it opens the title again ("" for
' none); ceilingKbps what it remembers of a slow network (0 for nothing).
' A quality is asked for converted only when it makes the file smaller,
' else the file plays as it is, as on the other apps.
function aqAsk(choice as string, again as string, ceilingKbps as float, file as dynamic) as object
    asked = { automatic: choice = "auto" or again <> "", quality: "original", lowered: false, mode: "auto", maxBitrate: 0, maxHeight: 0 }
    wanted = again
    if wanted = ""
        if choice = "auto"
            wanted = aqStartQuality(ceilingKbps)
            asked.lowered = wanted <> ""
        else if choice <> "original"
            wanted = choice
        end if
    end if
    if wanted = "" or wanted = "original" or aqFind(wanted) = invalid
        asked.lowered = false
        return asked
    end if
    if again = "" and not aqHas(aqOffered(aqFileKbps(file), aqFileLines(file)), wanted)
        ' Already under it: the file as it is.
        asked.lowered = false
        return asked
    end if
    asked.quality = wanted
    asked.mode = "transcode"
    asked.maxBitrate = aqKbps(wanted)
    asked.maxHeight = aqLines(wanted)
    return asked
end function

' The quality after a play issue and the ceiling it came from: { quality,
' ceiling }, or invalid when there is nothing lower. The ceiling is what
' the network carried (the bitrate playing, or the segments' speed when
' lower) times 0.75, never above the server's own quality for the file;
' the quality is the best under it, always lower than the one playing, 2
' Mbps 480p at least. Without a measure, the bitrate playing halves.
function aqStepDown(current as string, playingKbps as float, networkKbps as float, fileKbps as float, fileLines as integer, cap as string) as dynamic
    carried = 0.0
    if networkKbps > 0 and playingKbps > 0
        carried = networkKbps
        if playingKbps < carried then carried = playingKbps
    else if networkKbps > 0
        carried = networkKbps
    else if playingKbps > 0
        carried = playingKbps * 0.5
    end if
    below = playingKbps
    if current <> "original" then below = aqKbps(current)
    candidates = []
    for each id in aqLadder(fileKbps, fileLines, cap)
        if below <= 0 or aqKbps(id) < below then candidates.Push(id)
    end for
    if candidates.Count() = 0 then return invalid
    if carried <= 0
        ' Nothing known of the file or the network: a middling quality.
        chosen = candidates[0]
        if aqHas(candidates, "8m") then chosen = "8m"
        return { quality: chosen, ceiling: aqKbps(chosen) }
    end if
    ceiling = carried * 0.75
    for each id in candidates
        if aqKbps(id) <= ceiling then return { quality: id, ceiling: ceiling }
    end for
    return { quality: candidates[candidates.Count() - 1], ceiling: ceiling }
end function

' The next quality up and the bitrate it needs carried (before the
' headroom): { quality, kbps }, or invalid at the original or when the
' file's bitrate isn't known.
function aqStepUp(current as string, fileKbps as float, fileLines as integer, cap as string) as dynamic
    if current = "original" then return invalid
    up = []
    for each id in aqLadder(fileKbps, fileLines, cap)
        if aqKbps(id) > aqKbps(current) then up.Push(id)
    end for
    if up.Count() > 0 then return { quality: up[up.Count() - 1], kbps: aqKbps(up[up.Count() - 1]) }
    need = fileKbps
    if aqKbps(cap) > 0 then need = aqKbps(cap)
    if need <= 0 then return invalid
    return { quality: "original", kbps: need }
end function

' --- Players that go black between streams (docs/QUALITY.md) ---
'
' Roku's Video node replaces its content to play another stream, and the
' screen goes black meanwhile. Automatic moves to another stream only once
' the picture has stopped anyway (a wait of 2 s or more; at the original as
' the file is, one the network is to blame for, aqLongWait); a conversion whose
' bitrate changes as it is sent (PlaybackInfo.live_bitrate) steps its
' bitrate instead (PUT /api/playback/{id}/bitrate), keeping its size, and
' plays on.

' The least a stream of lines (as named, aqNamedLines) is stepped down to
' as it is sent: 8 Mbps for 4K, 3 Mbps for 1080p, 1.5 for 720p and 1
' below (docs/QUALITY.md).
function aqFloorKbps(lines as integer) as integer
    if lines > 1080 then return 8000
    if lines > 720 then return 3000
    if lines > 480 then return 1500
    return 1000
end function

' A bitrate stepped as it is sent, as the menu names qualities: "4 Mbps
' 1080p", "8 Mbps 4K" (lines 0: the bitrate alone).
function aqBitrateLabel(kbps as float, lines as integer) as string
    tenths = Int(kbps / 100)
    mbps = i18nNumber(tenths \ 10)
    if tenths MOD 10 <> 0 then mbps = i18nNumber(tenths / 10, 1)
    if lines > 0 then return aqPresetLabel(mbps, lines)
    return t("format.bitrate.mbps", { n: mbps })
end function

' The lines a stream is named by: a conversion's quality's (the server's
' own, cap, for the original converted), no more than the file's
' (fileNamed, aqNamedLines); the file's for the file as it is.
function aqStreamLines(quality as string, converted as boolean, cap as string, fileNamed as integer) as integer
    if not converted then return fileNamed
    lines = aqLines(quality)
    if quality = "original" then lines = aqLines(cap)
    if lines > 0 and fileNamed > 0 and fileNamed < lines then return fileNamed
    if lines > 0 then return lines
    return fileNamed
end function

' The bitrates a stream stepped down as it is sent climbs back by, one at a
' time, lowest first: the menu's above kbps, then its own.
function aqRungsUp(kbps as float, own as float) as object
    rungs = []
    for each id in ["2m", "4m", "8m", "12m", "15m", "20m", "30m"]
        if aqKbps(id) > kbps and aqKbps(id) < own then rungs.Push(aqKbps(id))
    end for
    rungs.Push(Int(own))
    return rungs
end function

' A decision of the watch's: kind "restart" (the title again at quality, a
' new stream; "converted" for the original converted at the server's own
' quality), "bitrate" (the stream goes on at kbps) or "next" (only where
' the next title starts changes); down when it is a step down to say;
' memory what the network's memory keeps for the next title (kbps, 0 to
' forget it, -1 to leave it).
function aqStep(kind as string, quality as string, kbps as float, down as boolean, memory as float) as object
    return { kind: kind, quality: quality, kbps: Int(kbps), down: down, memory: Int(memory + 0.5) }
end function

' --- The watch over one title ---

' now is the player's clock in ms; quality what plays ("original" or a
' quality's id), at playingKbps (0 unknown); the file's bitrate and lines;
' cap the server's own quality for it; lowered when it started under a
' remembered ceiling (counted as a step down). live: the stream is
' converted and its bitrate changes as it is sent; lines what it is named
' by; converted whether the video is converted (invalid: a quality is).
function aqWatch(now as integer, quality as string, playingKbps as float, fileKbps as float, fileLines as integer, cap as string, lowered as boolean, live = false as boolean, lines = 0 as integer, converted = invalid as dynamic) as object
    watch = {
        fileKbps: fileKbps,
        fileLines: fileLines,
        cap: cap,
        titleAt: now,
        changedAt: now,
        issueAt: now,
        downAt: -1,
        ceiling: 0.0,
        ' It settles: the last step up (its time and what it went to) until a
        ' step down or 10 minutes say whether it held, what didn't hold this
        ' title, and the calm a step up waits for.
        lastUpAt: -1,
        lastUpKey: "",
        tooHigh: {},
        calmMs: 180000,
        ' Where the next title starts, raised while this stream can't go
        ' higher ("" for where this one plays), and when it last was.
        nextTitle: "",
        raisedAt: -1,
        droppedTold: false,
        playingSince: -1,
        waitSince: -1,
        waitCounted: false,
        ' The wait under way lasted 2 s (aqLongWait): what's left of it is no
        ' short wait. It came on a network nothing measured, and is among
        ' originalWaits: when the original's long waits came on one.
        waitLong: false,
        waitUnmeasured: false,
        originalWaits: [],
        shortWaits: [],
        segments: [],
        lastPosition: -1,
        lastAt: -1
    }
    if lowered then watch.downAt = now
    aqStream(watch, quality, playingKbps, live, lines, converted)
    return watch
end function

' What plays now: quality at playingKbps, its own bitrate (which a step up
' goes back to at most), lines tall, its bitrate changing as it is sent
' when live.
sub aqStream(watch as object, quality as string, playingKbps as float, live as boolean, lines as integer, converted as dynamic)
    isConverted = quality <> "original"
    if GetInterface(converted, "ifBoolean") <> invalid then isConverted = converted
    watch.quality = quality
    watch.playingKbps = playingKbps
    watch.ownKbps = playingKbps
    watch.sentKbps = playingKbps
    watch.converted = isConverted
    watch.live = live and isConverted and playingKbps > 0
    watch.lines = lines
    ' The stream comes in several sizes the Video node moves between (the
    ' player sets it): its waits are the Video node's to answer.
    watch.adaptive = false
end sub

' Playing again at quality: a new stream of the same title.
sub aqRestarted(watch as object, now as integer, quality as string, playingKbps as float, live = false as boolean, lines = 0 as integer, converted = invalid as dynamic)
    aqStream(watch, quality, playingKbps, live, lines, converted)
    watch.changedAt = now
    watch.segments = []
    watch.lastPosition = -1
    aqSeeked(watch)
end sub

' Its bitrate stepped down as it is sent: below its own.
function aqStepped(watch as object) as boolean
    return watch.live and watch.playingKbps < watch.ownKbps
end function

' Below the original: a quality, or a bitrate stepped down.
function aqLowered(watch as object) as boolean
    return watch.quality <> "original" or aqStepped(watch)
end function

' The server's answer to a "bitrate" step to asked: what the stream goes on
' at (kbps), or 0 when it refused (a server or transcoder from before, a
' session gone). Refused, the stream is as it was, and moves as one whose
' bitrate can't change.
sub aqAnswered(watch as object, asked as float, kbps as float)
    if kbps <= 0
        watch.live = false
        watch.playingKbps = watch.sentKbps
        return
    end if
    watch.sentKbps = kbps
    if watch.playingKbps = asked then watch.playingKbps = kbps
end sub

' Where the video is (ms), as the Video node says each second: a jump
' ahead of the time gone by, or back, is a seek in Roku's own controls.
sub aqPosition(watch as object, now as integer, positionMs as integer)
    if watch.lastPosition >= 0
        moved = positionMs - watch.lastPosition
        if moved - (now - watch.lastAt) > 3000 or moved < -1000
            ' A wait in progress was the seek's; one now needs 2 s of playing first.
            watch.waitSince = -1
            watch.waitCounted = false
            watch.waitLong = false
            watch.waitUnmeasured = false
            if watch.playingSince >= 0 then watch.playingSince = now
        end if
    end if
    watch.lastPosition = positionMs
    watch.lastAt = now
end sub

' A seek, a start or a pause: the wait that follows isn't the network's.
sub aqSeeked(watch as object)
    watch.playingSince = -1
    watch.waitSince = -1
    watch.waitCounted = false
    watch.waitLong = false
    watch.waitUnmeasured = false
end sub

' The Video node's state changed. underrun is bufferingStatus.isUnderrun
' (invalid when the Roku doesn't say). Answers a step (aqStep), or invalid.
function aqState(watch as object, now as integer, state as string, underrun as dynamic) as dynamic
    issue = false
    waited = false
    if state = "playing"
        if watch.waitSince >= 0 and not watch.waitCounted
            if now - watch.waitSince >= 2000 and aqLongWait(watch, now)
                ' At the original, what arrived as it ended may show the network to blame after all.
                issue = true
                waited = true
            else if not watch.waitLong
                ' A short wait: two within a minute are a play issue.
                kept = [now]
                for each at in watch.shortWaits
                    if now - at <= 60000 then kept.Push(at)
                end for
                watch.shortWaits = kept
                if kept.Count() >= 2
                    watch.shortWaits = []
                    issue = true
                end if
            end if
        end if
        watch.waitSince = -1
        watch.waitLong = false
        watch.waitUnmeasured = false
        if watch.playingSince < 0 then watch.playingSince = now
    else if state = "buffering"
        ' Only after 2 s of playing (not a start or a seek), and not when
        ' the Roku says it isn't for want of data.
        flowing = watch.playingSince >= 0 and now - watch.playingSince >= 2000
        notUnderrun = GetInterface(underrun, "ifBoolean") <> invalid and not underrun
        if flowing and not notUnderrun and watch.waitSince < 0
            watch.waitSince = now
            watch.waitCounted = false
            watch.waitLong = false
            watch.waitUnmeasured = false
        end if
        watch.playingSince = -1
    else
        aqSeeked(watch)
    end if
    if issue then return aqIssue(watch, now, waited)
    return invalid
end function

' A segment the Video node downloaded: bytes in ms.
sub aqSegment(watch as object, now as integer, bytes as float, ms as float)
    if bytes <= 0 or ms <= 0 then return
    kept = []
    for each segment in watch.segments
        if now - segment.at <= 240000 then kept.Push(segment)
    end for
    kept.Push({ at: now, bytes: bytes, ms: ms })
    watch.segments = kept
end sub

' What the network carried, in kbps, over the segments since `since`
' (their bytes over the time they took); 0 with fewer than `least`.
function aqCarried(watch as object, since as integer, least as integer) as float
    bytes = 0.0
    ms = 0.0
    count = 0
    for each segment in watch.segments
        if segment.at >= since
            bytes = bytes + segment.bytes
            ms = ms + segment.ms
            count = count + 1
        end if
    end for
    if count < least or ms <= 0 then return 0
    return bytes * 8 / ms
end function

' Once a second while it plays or waits: a wait of 2 s is a play issue,
' and a quiet network may bring the bitrate, or where the next title
' starts, back up. Answers a step (aqStep), or invalid.
function aqTick(watch as object, now as integer) as dynamic
    if watch.waitSince >= 0 and not watch.waitCounted and now - watch.waitSince >= 2000
        if aqLongWait(watch, now)
            watch.waitCounted = true
            return aqIssue(watch, now, true)
        end if
    end if
    if watch.playingSince < 0 then return invalid
    return aqUp(watch, now)
end function

' Whether the wait under way, 2 s long by now, is a play issue. At the
' original as the file is, only on a network the segments of the last 2
' minutes show carrying less than 1.5 times the file's bitrate: on one that
' carries more, the server is why it waits (its disk, a remux catching up),
' and a conversion reads the same file and does more. Where no segments
' came (a direct play), the second such wait within 5 minutes. Asked again
' as the wait goes on and as it ends, with the segments since.
function aqLongWait(watch as object, now as integer) as boolean
    watch.waitLong = true
    if watch.quality <> "original" or watch.converted then return true
    network = aqCarried(watch, now - 120000, 2)
    if network > 0 and watch.fileKbps > 0 then return network < 1.5 * watch.fileKbps
    if not watch.waitUnmeasured
        watch.waitUnmeasured = true
        kept = [now]
        for each at in watch.originalWaits
            if now - at < 300000 then kept.Push(at)
        end for
        watch.originalWaits = kept
    end if
    return watch.originalWaits.Count() >= 2
end function

' A play issue: a step down, as the stream is sent when it can be; else
' another stream, but only once the picture has stopped anyway (waited:
' a wait of 2 s or more), as the screen goes black between streams.
function aqIssue(watch as object, now as integer, waited as boolean) as dynamic
    watch.issueAt = now
    if watch.adaptive or now - watch.changedAt < 20000 then return invalid
    network = aqCarried(watch, now - 20000, 1)
    if watch.live then return aqLowerBitrate(watch, now, network)
    ' Two short waits: the picture plays on.
    if not waited then return invalid
    down = aqStepDown(watch.quality, watch.playingKbps, network, watch.fileKbps, watch.fileLines, watch.cap)
    if down = invalid then return invalid
    aqWentDown(watch, now)
    watch.ceiling = down.ceiling
    ' The lowest quality may be above the ceiling: that is where the next title starts.
    memory = down.ceiling
    if aqKbps(down.quality) > memory then memory = aqKbps(down.quality)
    return aqStep("restart", down.quality, 0, true, memory)
end function

' The stream goes on lower, its size kept: what the network carried (the
' bitrate playing, or the segments' speed when lower; half the bitrate
' without them) times 0.75, rounded down to whole Mbps as on Samsung and
' LG, no lower than its size's floor (where the network waits).
function aqLowerBitrate(watch as object, now as integer, networkKbps as float) as dynamic
    kbps = watch.playingKbps
    carried = kbps * 0.5
    if networkKbps > 0
        carried = networkKbps
        if kbps < carried then carried = kbps
    end if
    ceiling = carried * 0.75
    target = Int(ceiling / 1000) * 1000
    if target > watch.ownKbps then target = watch.ownKbps
    least = aqFloorKbps(watch.lines)
    if least > watch.ownKbps then least = watch.ownKbps
    if target < least then target = least
    if target >= kbps then return invalid
    aqWentDown(watch, now)
    watch.ceiling = ceiling
    watch.playingKbps = target
    memory = ceiling
    if target > memory then memory = target
    return aqStep("bitrate", watch.quality, target, true, memory)
end function

' A step down: one soon after a step up says that step was one too far, not
' tried again this title, and the calm before the next one doubles (to 12
' minutes at most). Where the next title starts goes down with it.
sub aqWentDown(watch as object, now as integer)
    if watch.lastUpAt >= 0 and now - watch.lastUpAt < 600000
        watch.tooHigh[watch.lastUpKey] = true
        watch.calmMs = watch.calmMs * 2
        if watch.calmMs > 720000 then watch.calmMs = 720000
    end if
    watch.lastUpAt = -1
    watch.lastUpKey = ""
    watch.nextTitle = ""
    watch.changedAt = now
    watch.downAt = now
end sub

' Long enough without an issue and since `since`, and since a step down.
function aqCalm(watch as object, now as integer, since as integer) as boolean
    if now - watch.issueAt < watch.calmMs or now - since < watch.calmMs then return false
    return watch.downAt < 0 or now - watch.downAt >= 300000
end function

' After a calm spell, when the segments since (20 s of video at least)
' carried 1.6 times it: a bitrate stepped down climbs back one step, as it
' is sent. A stream that can't go higher without another (which would
' blank the screen while it plays well) stays; the next title starts
' higher instead.
function aqUp(watch as object, now as integer) as dynamic
    since = watch.issueAt
    if watch.changedAt > since then since = watch.changedAt
    if watch.raisedAt > since then since = watch.raisedAt
    if aqStepped(watch)
        if not aqCalm(watch, now, watch.changedAt) then return invalid
        up = aqRungsUp(watch.playingKbps, watch.ownKbps)[0]
        key = up.ToStr() + " kbps"
        if watch.tooHigh.DoesExist(key) then return invalid
        carried = aqCarried(watch, since, 10)
        if carried <= 0 or carried < 1.6 * up then return invalid
        watch.playingKbps = up
        watch.changedAt = now
        watch.lastUpAt = now
        watch.lastUpKey = key
        memory = up
        if up >= watch.ownKbps
            ' Back at its own: the quality's, or no limit for the original.
            memory = aqKbps(watch.quality)
            if watch.quality = "original" then memory = 0
        end if
        return aqStep("bitrate", watch.quality, up, false, memory)
    end if
    ' Frames dropped lowered where the next title starts: not raised again.
    if watch.droppedTold then return invalid
    calmFrom = watch.changedAt
    if watch.raisedAt > calmFrom then calmFrom = watch.raisedAt
    if not aqCalm(watch, now, calmFrom) then return invalid
    current = watch.nextTitle
    if current = "" then current = watch.quality
    up = aqStepUp(current, watch.fileKbps, watch.fileLines, watch.cap)
    if up = invalid then return invalid
    if watch.tooHigh.DoesExist(up.quality) then return invalid
    carried = aqCarried(watch, since, 10)
    if carried <= 0 or carried < 1.6 * up.kbps then return invalid
    watch.nextTitle = up.quality
    watch.raisedAt = now
    watch.lastUpAt = now
    watch.lastUpKey = up.quality
    memory = aqKbps(up.quality)
    return aqStep("next", up.quality, 0, false, memory)
end function

' The device dropped frames (for a Roku that would say: none does yet). In
' the title's first minute, a guess that it can't keep up with this stream:
' another (the original converted at the server's own quality, a
' conversion a quality lower). Later, only where the next title starts goes
' a quality lower, once.
function aqDropped(watch as object, now as integer) as dynamic
    lower = aqStepDown(watch.quality, watch.playingKbps, watch.playingKbps, watch.fileKbps, watch.fileLines, watch.cap)
    quality = "converted"
    if watch.quality <> "original" or watch.converted
        quality = ""
        if lower <> invalid then quality = lower.quality
    end if
    if now - watch.titleAt < 60000
        if quality = "" or now - watch.changedAt < 20000 then return invalid
        watch.issueAt = now
        aqWentDown(watch, now)
        return aqStep("restart", quality, 0, quality <> "converted", -1)
    end if
    if lower = invalid or watch.droppedTold then return invalid
    watch.droppedTold = true
    watch.nextTitle = ""
    return aqStep("next", lower.quality, 0, false, aqKbps(lower.quality))
end function

' --- Remembered on this Roku ---

' The ceiling Automatic remembers for server (kbps), 0 past 30 minutes.
' atSeconds is the wall clock; section a roRegistrySection or a stand-in.
function aqRemembered(section as object, server as string, atSeconds as integer) as float
    found = httpObject(httpJson(section.Read("ceilings")))
    if found = invalid then return 0
    entry = httpObject(found[server])
    if entry = invalid then return 0
    at = httpNumber(entry.at, 0)
    if atSeconds - at > 1800 or at - atSeconds > 60 then return 0
    return httpNumber(entry.kbps, 0)
end function

' Remembers kbps for server (0 forgets it).
sub aqRemember(section as object, server as string, kbps as float, atSeconds as integer)
    found = httpObject(httpJson(section.Read("ceilings")))
    kept = {}
    if found <> invalid
        ' Only those still in force: the registry is small.
        for each key in found
            entry = httpObject(found[key])
            if entry <> invalid and atSeconds - httpNumber(entry.at, 0) <= 1800 then kept[key] = entry
        end for
    end if
    if kbps > 0
        kept[server] = { kbps: Int(kbps), at: atSeconds }
    else
        kept.Delete(server)
    end if
    section.Write("ceilings", FormatJson(kept))
    section.Flush()
end sub

' The quality setting, in the same section.
function aqChoice(section as object) as string
    return aqFromSetting(section.Read("quality"))
end function

sub aqChoose(section as object, id as string)
    section.Write("quality", aqFromSetting(id))
    section.Flush()
end sub

' --- Where Automatic starts: what the server knows of the network ---
'
' Before a title starts in Automatic the player asks the server what it
' knows of this Roku's network (GET /api/playback/network, 1.5 s at most);
' its ceiling takes the place of the one remembered here, which counts
' only without an answer. Where the server asks for a speed test, the API
' task fetches 8 MiB of GET /api/playback/speedtest whole (roUrlTransfer
' gives nothing of a body before its end) and times the whole request;
' the result goes to the server with probe_complete false, and its answer
' is the ceiling.

function aqProbeBytes() as integer
    return 8388608
end function

' The answer's ceiling in kbps, 0 for no limit.
function aqNetworkCeiling(answer as dynamic) as float
    if GetInterface(answer, "ifAssociativeArray") = invalid then return 0
    kbps = httpNumber(answer.ceiling_kbps, 0)
    if kbps <= 0 then return 0
    return kbps
end function

' Whether the server asks for a speed test first (never on its own network).
function aqWantsProbe(answer as dynamic) as boolean
    if GetInterface(answer, "ifAssociativeArray") = invalid then return false
    if httpTrue(answer.local) then return false
    return httpTrue(answer.probe)
end function

' What a speed test of bytes in ms tells the server: { probe_kbps,
' probe_complete }, or invalid under 256 KiB (nothing is known then).
' The whole request is timed, so a Roku never says it came whole in time.
function aqProbeResult(bytes as float, ms as float) as dynamic
    if bytes < 262144 or ms <= 0 then return invalid
    kbps = Int(bytes * 8 / ms + 0.5)
    if kbps < 1 then kbps = 1
    if kbps > 10000000 then kbps = 10000000
    return { probe_kbps: kbps, probe_complete: false }
end function

' The ceiling a speed test gives when the server didn't keep it: 70 % of
' what it measured, as the server counts it.
function aqProbeCeiling(result as dynamic) as float
    if result = invalid then return 0
    if httpTrue(result.probe_complete) then return 0
    return result.probe_kbps * 0.7
end function

' How long the speed test may take, in ms, after asking took askedMs: the
' whole of it holds the play back about 3.5 s at most.
function aqProbeTimeoutMs(askedMs as integer) as integer
    left = 3000 - askedMs
    if left < 1500 then left = 1500
    return left
end function

' The server's answers kept in the API task for each sign-in (server and
' profile): { at (ms), answer }, 10 minutes, so a play on the home
' network doesn't wait for them. A 404 (a server from before) is kept too.
function aqNetworkKept(kept as object, key as string, nowMs as integer) as dynamic
    entry = kept[key]
    if entry = invalid then return invalid
    if nowMs - entry.at > 600000 or nowMs < entry.at
        kept.Delete(key)
        return invalid
    end if
    return entry.answer
end function

sub aqNetworkKeep(kept as object, key as string, answer as object, nowMs as integer)
    if answer.ok and GetInterface(answer.data, "ifAssociativeArray") = invalid then return
    if not answer.ok and answer.status <> 404 then return
    kept[key] = { at: nowMs, answer: answer }
end sub

' PUT /api/playback/network's body as JSON (ceiling_kbps null forgets):
' { ceilingKbps } or { probeKbps } from the screens.
function aqTellBody(clientId as string, spec as object) as string
    if httpNumber(spec.probeKbps, 0) > 0
        return FormatJson({ client_id: clientId, probe_kbps: Int(spec.probeKbps), probe_complete: false })
    end if
    kbps = Int(httpNumber(spec.ceilingKbps, 0) + 0.5)
    if kbps <= 0 then return "{""client_id"":" + FormatJson(clientId) + ",""ceiling_kbps"":null}"
    return FormatJson({ client_id: clientId, ceiling_kbps: kbps })
end function
