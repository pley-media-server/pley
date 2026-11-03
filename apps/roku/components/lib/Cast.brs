' Being cast to (docs/API.md, "Casting to another device"): the profile's
' phones and browsers send this Roku a title and steer its player, through
' its event stream, and it tells them what it plays. Roku is cast to and
' doesn't cast. Here what needs no SceneGraph: the device's id and name, the
' stream's query, the commands as they come and the reports.

' This Roku's id for casting, made once and kept in section (the registry
' section `device`): 32 hex digits. uuid is a new random one, for the
' first time.
function castDeviceId(section as object, uuid as string) as string
    id = section.Read("cast")
    if castValidId(id) then return id
    id = LCase(uuid.Replace("-", ""))
    section.Write("cast", id)
    section.Flush()
    return id
end function

' What the server takes for an id: 8 to 64 letters, digits, - and _.
function castValidId(id as string) as boolean
    return CreateObject("roRegex", "^[A-Za-z0-9_-]{8,64}$", "").IsMatch(id)
end function

' What people call this Roku: the name it was given in Roku's settings
' (friendly), else its model's, else "Roku"; 64 characters at most.
function castName(friendly as dynamic, model as dynamic) as string
    for each name in [friendly, model]
        clean = httpClean(name, 64, false)
        if clean <> "" then return clean
    end for
    return "Roku"
end function

' The event stream's query that lists this Roku for casting while it is
' open ("?cast_device=…&cast_name=…&cast_platform=roku"); "" without cast
' ({ device, name }), or with an id the server wouldn't take.
function castQuery(cast as dynamic) as string
    data = httpObject(cast)
    if data = invalid then return ""
    device = httpString(data.device)
    name = httpClean(data.name, 64, false)
    if not castValidId(device) or name = "" then return ""
    return "?cast_device=" + urlEncode(device) + "&cast_name=" + urlEncode(name) + "&cast_platform=roku"
end function

' A command from the event stream for this Roku (deviceId), or invalid for
' any other event, another device's, or one that doesn't say enough:
' { kind: "play", itemId, startMs, fileId? }, startMs -1 for where the
' profile left off; or { kind: "control", action, positionMs }, action
' "pause", "resume", "seek" or "stop", positionMs -1 but for a seek.
function castCommand(event as object, deviceId as string) as dynamic
    kind = httpString(event.type)
    if kind <> "cast_play" and kind <> "cast_control" then return invalid
    if deviceId = "" or httpString(event.device_id) <> deviceId then return invalid
    if kind = "cast_play"
        itemId = httpNumber(event.item_id, 0)
        if itemId <= 0 then return invalid
        command = { kind: "play", itemId: itemId, startMs: castMs(event.position_ms) }
        fileId = httpNumber(event.file_id, 0)
        if fileId > 0 then command.fileId = fileId
        return command
    end if
    action = httpString(event.action)
    if action <> "pause" and action <> "resume" and action <> "seek" and action <> "stop" then return invalid
    positionMs = castMs(event.position_ms)
    if action = "seek" and positionMs < 0 then return invalid
    if action <> "seek" then positionMs = -1
    return { kind: "control", action: action, positionMs: positionMs }
end function

' A position in ms, or -1 for none.
function castMs(value as dynamic) as integer
    ms = httpNumber(value, -1)
    if ms < 0 then return -1
    return Int(ms)
end function

' Whether the screen on top (its route) takes commands: not while signed
' out, signing in, or choosing a profile or typing its PIN.
function castTakesCommands(route as string, signedIn as boolean) as boolean
    if not signedIn then return false
    return route <> "" and route <> "link" and route <> "servers" and route <> "profiles"
end function

' Whether a title sent takes the place of the screen on top: a player
' leaves, as Back would leave it, so Back after the title goes where it
' would have gone.
function castReplaces(route as string) as boolean
    return route = "player" or route = "livePlayer"
end function

' The player's params for a play command (castCommand's).
function castPlayerParams(command as object) as object
    params = { itemId: command.itemId, startMs: command.startMs }
    if command.fileId <> invalid then params.fileId = command.fileId
    return params
end function

' What the Video node's state is to the devices casting: "paused",
' "buffering" or "playing".
function castPlayState(videoState as string) as string
    if videoState = "paused" then return "paused"
    if videoState = "buffering" then return "buffering"
    return "playing"
end function

function castStatePath(deviceId as string) as string
    return "/api/cast/devices/" + urlEncode(deviceId) + "/state"
end function

' The body of PUT /api/cast/devices/{id}/state, as JSON text: what plays,
' where and how; itemId 0 (or invalid) for nothing.
function castStateBody(itemId as dynamic, positionMs as dynamic, durationMs as dynamic, state as string) as string
    id = httpNumber(itemId, 0)
    if id <= 0 then return "{""item_id"":null}"
    position = Int(httpNumber(positionMs, 0))
    if position < 0 then position = 0
    body = { item_id: id, position_ms: position, state: state }
    duration = Int(httpNumber(durationMs, 0))
    if duration > 0 then body.duration_ms = duration
    return FormatJson(body)
end function
