' What every request to a pley server carries, and how its answers read.

' pley for Roku's client ID (docs/API.md, "pley's own apps"): it signs in
' with a TV code only.
function httpClientId() as string
    return "https://pley-media-server.github.io/pley/oauth/roku.json"
end function

' What the app asks to be allowed: browsing, playing, the profile's
' playlists and switching to the household's other profiles, as Kodi; and
' where the server has them, requests and approvals (authScopeFor).
function httpScope() as string
    return "library play playlists profiles"
end function

' "pleyRoku/0.1.0 (Roku OS 15.0.4; Roku Ultra)". os is roDeviceInfo's
' GetOSVersion(): { major, minor, revision, build }.
function httpUserAgent(appVersion as string, os as dynamic, model as string) as string
    osVersion = httpOsVersion(os)
    if osVersion = "" then osVersion = "?"
    name = ""
    for i = 1 to Len(model)
        c = Mid(model, i, 1)
        code = Asc(c)
        if code >= 32 and code <> 127 and c <> "(" and c <> ")" and c <> ";" then name = name + c
    end for
    name = name.Trim()
    if name = "" then name = "Roku"
    version = appVersion.Trim()
    if version = "" then version = "0"
    return "pleyRoku/" + version + " (Roku OS " + osVersion + "; " + name + ")"
end function

' "15.0.4" from GetOSVersion()'s { major, minor, revision, build }; "" when
' it says nothing.
function httpOsVersion(os as dynamic) as string
    if GetInterface(os, "ifAssociativeArray") = invalid then return ""
    parts = []
    for each key in ["major", "minor", "revision"]
        value = urlValue(os[key])
        if value <> "" then parts.Push(value)
    end for
    return parts.Join(".")
end function

' Text from a server or a page, safe to show: one line without control or
' direction characters, at most 200 characters (a message, a name).
function httpSentence(text as dynamic) as string
    return httpClean(text, 200, false)
end function

' text without control characters (line breaks kept with keepLines) and
' without the invisible characters that turn or hide text (direction
' marks and overrides, zero-width ones), at most max characters. Native
' calls only: it runs for every title of every list.
function httpClean(text as dynamic, max as integer, keepLines as boolean) as string
    if GetInterface(text, "ifString") = invalid then return ""
    cache = httpCache()
    out = Left(text, max + 40)
    ' Only text beyond ASCII can hold them.
    if cache.nonAscii.IsMatch(out)
        for each hidden in cache.hidden
            out = out.Replace(hidden, "")
        end for
    end if
    if keepLines
        out = cache.controlsButLines.ReplaceAll(out, "")
    else
        out = cache.controls.ReplaceAll(out, " ")
    end if
    out = out.Trim()
    if Len(out) > max then out = Left(out, max).Trim()
    return out
end function

' What httpClean and urlEncode use, made once per thread.
function httpCache() as object
    globals = GetGlobalAA()
    if globals.httpCache = invalid
        hidden = []
        for each range in [[&h200B, &h200F], [&h202A, &h202E], [&h2060, &h2064], [&h2066, &h2069], [&hFEFF, &hFEFF]]
            for code = range[0] to range[1]
                hidden.Push(Chr(code))
            end for
        end for
        globals.httpCache = {
            hidden: hidden,
            nonAscii: CreateObject("roRegex", "[^\x00-\x7F]", ""),
            controls: CreateObject("roRegex", "[\x00-\x1F\x7F]+", ""),
            controlsButLines: CreateObject("roRegex", "[\x00-\x09\x0B-\x1F\x7F]", ""),
            unreserved: CreateObject("roRegex", "^[A-Za-z0-9._~\-]*$", "")
        }
    end if
    return globals.httpCache
end function

' Whether a WWW-Authenticate value is a Bearer challenge with
' error="invalid_token" (RFC 6750 §3): the token is expired, signed out or
' unknown, and a refresh may help.
function httpInvalidToken(header as dynamic) as boolean
    if GetInterface(header, "ifString") = invalid then return false
    h = header.Trim()
    if LCase(Left(h, 6)) <> "bearer" then return false
    if Len(h) > 6
        nextChar = Mid(h, 7, 1)
        if nextChar <> " " and nextChar <> Chr(9) then return false
    end if
    re = CreateObject("roRegex", "(^|[\s,])error\s*=\s*""?invalid_token""?(\s*,|\s*$)", "i")
    return re.IsMatch(Mid(h, 7))
end function

' A response as the screens get it:
' { ok, status, data, error, message }. data is the JSON answer (or
' invalid); error is the server's code (pley's "code" or OAuth's "error"),
' or "network" when the server didn't answer: nothing did, or a proxy in
' front of it answered for it (502, 504, or a 503 that isn't pley's);
' message is a sentence for people.
function httpAnswer(status as integer, body as dynamic) as object
    result = { ok: status >= 200 and status < 300, status: status, data: invalid, error: "", message: "" }
    result.data = httpJson(body)
    if result.ok then return result
    data = result.data
    fromPley = GetInterface(data, "ifAssociativeArray") <> invalid and (data.DoesExist("code") or data.DoesExist("error"))
    if status <= 0 or status = 502 or status = 504 or (status = 503 and not fromPley)
        result.error = "network"
        result.message = t("common.errors.unreachable")
        return result
    end if
    if GetInterface(data, "ifAssociativeArray") <> invalid
        result.error = httpString(data.code)
        if result.error = "" then result.error = httpString(data.error)
        result.message = httpSentence(data.message)
        if result.message = "" then result.message = httpSentence(data.error_description)
    end if
    if result.message = "" then result.message = httpStatusSentence(status)
    return result
end function

' What a status says when the answer doesn't.
function httpStatusSentence(status as integer) as string
    if status = 401 then return t("common.errors.signedOut")
    if status = 403 then return t("common.errors.forbidden")
    if status = 404 then return t("common.errors.gone")
    if status = 410 then return t("common.errors.stopped")
    if status = 429 then return t("common.errors.busy")
    return t("common.errors.status", { status: status })
end function

' text parsed as JSON, or invalid. Only text that looks like JSON is
' parsed: ParseJson logs an error for anything else (an HTML error page).
function httpJson(text as dynamic) as dynamic
    if GetInterface(text, "ifString") = invalid then return invalid
    start = Left(text.Trim(), 1)
    if start <> "{" and start <> "[" then return invalid
    return ParseJson(text)
end function

' A JSON string value, or "".
function httpString(value as dynamic) as string
    if GetInterface(value, "ifString") = invalid then return ""
    return value
end function

' Whether a JSON value is true (false for anything but a boolean).
function httpTrue(value as dynamic) as boolean
    if GetInterface(value, "ifBoolean") = invalid then return false
    return value
end function

' A JSON number value (Integer, LongInteger, Float or Double), or fallback.
function httpNumber(value as dynamic, fallback as dynamic) as dynamic
    if GetInterface(value, "ifInt") <> invalid or GetInterface(value, "ifLongInt") <> invalid then return value
    if GetInterface(value, "ifFloat") <> invalid or GetInterface(value, "ifDouble") <> invalid then return value
    return fallback
end function

' A JSON array value, or [].
function httpArray(value as dynamic) as object
    if GetInterface(value, "ifArray") = invalid then return []
    return value
end function

' A JSON object value, or invalid.
function httpObject(value as dynamic) as dynamic
    if GetInterface(value, "ifAssociativeArray") = invalid then return invalid
    return value
end function
