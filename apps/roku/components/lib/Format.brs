' Times and progress as the screens write them. Durations and positions
' are milliseconds, as the server sends them.

' "1:02:03", or "12:34" under an hour.
function fmtClock(ms as dynamic) as string
    total = Int(httpNumber(ms, 0) / 1000)
    if total < 0 then total = 0
    hours = Int(total / 3600)
    minutes = Int((total mod 3600) / 60)
    seconds = total mod 60
    if hours > 0 then return hours.ToStr() + ":" + fmtTwo(minutes) + ":" + fmtTwo(seconds)
    return minutes.ToStr() + ":" + fmtTwo(seconds)
end function

function fmtTwo(value as integer) as string
    if value < 10 then return "0" + value.ToStr()
    return value.ToStr()
end function

' "1 h 42 min", "42 min"; "" without a duration.
function fmtDuration(ms as dynamic) as string
    total = httpNumber(ms, 0)
    if total <= 0 then return ""
    minutes = Int((total + 30000) / 60000)
    if minutes < 1 then minutes = 1
    hours = Int(minutes / 60)
    minutes = minutes mod 60
    if hours = 0 then return minutes.ToStr() + " min"
    if minutes = 0 then return hours.ToStr() + " h"
    return hours.ToStr() + " h " + minutes.ToStr() + " min"
end function

' "42 min left"; "" without a duration.
function fmtLeft(positionMs as dynamic, durationMs as dynamic) as string
    remaining = httpNumber(durationMs, 0) - httpNumber(positionMs, 0)
    if httpNumber(durationMs, 0) <= 0 or remaining <= 0 then return ""
    return fmtDuration(remaining) + " left"
end function

' How far along, from 0 to 1.
function fmtProgress(positionMs as dynamic, durationMs as dynamic) as float
    duration = httpNumber(durationMs, 0)
    position = httpNumber(positionMs, 0)
    if duration <= 0 or position <= 0 then return 0
    if position >= duration then return 1
    return position / duration
end function

' "S1 E2"; "E2" without a season; "" without an episode.
function fmtEpisode(season as dynamic, episode as dynamic) as string
    s = httpNumber(season, -1)
    e = httpNumber(episode, -1)
    if e < 0 then return ""
    if s < 0 then return "E" + Int(e).ToStr()
    return "S" + Int(s).ToStr() + " E" + Int(e).ToStr()
end function

' "Resume from 12:34".
function fmtResume(ms as dynamic) as string
    return "Resume from " + fmtClock(ms)
end function

' Whether a title has a place to resume from: started, not near its end.
function fmtResumable(positionMs as dynamic, durationMs as dynamic) as boolean
    position = httpNumber(positionMs, 0)
    if position < 1000 then return false
    duration = httpNumber(durationMs, 0)
    if duration <= 0 then return true
    return position < duration * 0.9
end function

' The line under a title on its details: "2019 · 1 h 42 min · PG-13 · ★ 7.5".
function fmtMeta(item as object) as string
    parts = []
    year = httpNumber(item.year, 0)
    if year > 0 then parts.Push(Int(year).ToStr())
    duration = fmtDuration(item.duration_ms)
    if duration <> "" then parts.Push(duration)
    rating = httpSentence(item.content_rating)
    if rating <> "" then parts.Push(rating)
    score = httpNumber(item.rating, 0)
    if score > 0 then parts.Push("★ " + fmtOneDecimal(score))
    return parts.Join(" · ")
end function

' "1 movie", "245 movies", "31 shows", "12 artists" for a library's kind.
function fmtCount(count as dynamic, kind as string) as string
    n = Int(httpNumber(count, 0))
    noun = "title"
    if kind = "movies" then noun = "movie"
    if kind = "shows" then noun = "show"
    if kind = "music" then noun = "artist"
    if n = 1 then return "1 " + noun
    return n.ToStr() + " " + noun + "s"
end function

' 7.5, 8 (no ".0").
function fmtOneDecimal(value as dynamic) as string
    tenths = Int(value * 10 + 0.5)
    if tenths mod 10 = 0 then return Int(tenths / 10).ToStr()
    return Int(tenths / 10).ToStr() + "." + (tenths mod 10).ToStr()
end function

' The languages offered for the audio and subtitle defaults, as ISO 639-2/B
' codes (the other apps' 34): [[code, name]].
function fmtLanguages() as object
    return [
        ["ara", "Arabic"], ["bul", "Bulgarian"], ["cat", "Catalan"], ["chi", "Chinese"], ["hrv", "Croatian"], ["cze", "Czech"], ["dan", "Danish"],
        ["dut", "Dutch"], ["eng", "English"], ["fin", "Finnish"], ["fre", "French"], ["ger", "German"], ["gre", "Greek"], ["heb", "Hebrew"],
        ["hin", "Hindi"], ["hun", "Hungarian"], ["ind", "Indonesian"], ["ita", "Italian"], ["jpn", "Japanese"], ["kor", "Korean"], ["nor", "Norwegian"],
        ["per", "Persian"], ["pol", "Polish"], ["por", "Portuguese"], ["rum", "Romanian"], ["rus", "Russian"], ["srp", "Serbian"], ["slo", "Slovak"],
        ["spa", "Spanish"], ["swe", "Swedish"], ["tha", "Thai"], ["tur", "Turkish"], ["ukr", "Ukrainian"], ["vie", "Vietnamese"]
    ]
end function

' A language's name, its code for one not offered, none for "". Files name
' some by their ISO 639-2/T code (fra for fre).
function fmtLanguageName(code as string, none as string) as string
    if code = "" then return none
    wanted = LCase(code)
    other = { fra: "fre", deu: "ger", zho: "chi", ces: "cze", nld: "dut", ell: "gre", fas: "per", ron: "rum", slk: "slo" }
    if other.DoesExist(wanted) then wanted = other[wanted]
    for each language in fmtLanguages()
        if language[0] = wanted then return language[1]
    end for
    return code
end function

' --- Resolutions and dynamic ranges ---
' A library's filters (lib/Library.brs) and smart filters (lib/Pages.brs)
' name them by the server's tokens (docs/API.md, GET
' /api/libraries/{id}/items).

' The resolutions, best first, as the other apps name them: [[token,
' label]]. 1080 holds 1440p too.
function fmtResolutions() as object
    return [["4k", "4K"], ["1080", "1080p"], ["720", "720p"], ["sd", "SD"]]
end function

' The dynamic ranges, best first; "hdr" is any of them but SDR.
function fmtDynamicRanges() as object
    return [["hdr", "HDR"], ["dolby_vision", "Dolby Vision"], ["hdr10plus", "HDR10+"], ["hdr10", "HDR10"], ["hlg", "HLG"], ["sdr", "SDR"]]
end function

' What table ([[token, label]]) calls token; "" for one it doesn't hold.
function fmtTokenLabel(table as object, token as dynamic) as string
    wanted = httpString(token)
    for each pair in table
        if pair[0] = wanted then return pair[1]
    end for
    return ""
end function

' --- Times to come and gone (requests' words, lib/Requests.brs) ---
' Roku has no relative-time formatter: these are the app's own, in its
' words. Times are unix seconds; zone is what fmtZoneSeconds gives.

' The seconds to add to a UTC time for this Roku's local time (its time
' zone, summer time as it is now).
function fmtZoneSeconds() as integer
    date = CreateObject("roDateTime")
    utc = date.AsSeconds()
    date.ToLocalTime()
    return date.AsSeconds() - utc
end function

' How far ahead at is: "in a moment", "in 20 min", "in 1 hour", "in 3
' hours", "tomorrow", "in 3 days".
function fmtUntil(at as dynamic, now as integer) as string
    diff = httpNumber(at, 0) - now
    if diff < 60 then return "in a moment"
    minutes = Int((diff + 30) / 60)
    if minutes < 60 then return "in " + urlValue(minutes) + " min"
    hours = Int((diff + 1800) / 3600)
    if hours < 24
        if hours = 1 then return "in 1 hour"
        return "in " + urlValue(hours) + " hours"
    end if
    days = Int((diff + 43200) / 86400)
    if days <= 1 then return "tomorrow"
    return "in " + urlValue(days) + " days"
end function

' How long ago at was: "now", "12 min ago", "1 hour ago", "3 hours ago",
' "yesterday", "3 days ago", else its day ("Mar 4, 2026").
function fmtAgo(at as dynamic, now as integer, zone as integer) as string
    seconds = httpNumber(at, 0)
    diff = now - seconds
    if diff < 60 then return "now"
    if diff < 3600 then return urlValue(Int(diff / 60)) + " min ago"
    if diff < 86400
        hours = Int(diff / 3600)
        if hours = 1 then return "1 hour ago"
        return urlValue(hours) + " hours ago"
    end if
    days = Int(diff / 86400)
    if days = 1 then return "yesterday"
    if days < 30 then return urlValue(days) + " days ago"
    return fmtMonthDay(seconds, zone, true)
end function

function fmtMonths() as object
    return ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
end function

' at's day in local time: "Nov 4", with its year ("Nov 4, 2027") when
' withYear.
function fmtMonthDay(at as dynamic, zone as integer, withYear as boolean) as string
    date = CreateObject("roDateTime")
    date.FromSeconds(Int(httpNumber(at, 0) + zone))
    text = fmtMonths()[date.GetMonth() - 1] + " " + urlValue(date.GetDayOfMonth())
    if withYear then text = text + ", " + urlValue(date.GetYear())
    return text
end function

' at's day in local time: "Nov 4" in the year now is in, else "Nov 4,
' 2027".
function fmtDay(at as dynamic, now as integer, zone as integer) as string
    return fmtMonthDay(at, zone, fmtYear(at, zone) <> fmtYear(now, zone))
end function

function fmtYear(at as dynamic, zone as integer) as integer
    date = CreateObject("roDateTime")
    date.FromSeconds(Int(httpNumber(at, 0) + zone))
    return date.GetYear()
end function

' at's time of day in local time: "9:05 PM".
function fmtTimeOfDay(at as dynamic, zone as integer) as string
    date = CreateObject("roDateTime")
    date.FromSeconds(Int(httpNumber(at, 0) + zone))
    hours = date.GetHours()
    suffix = " AM"
    if hours >= 12 then suffix = " PM"
    hours = hours mod 12
    if hours = 0 then hours = 12
    return urlValue(hours) + ":" + fmtTwo(date.GetMinutes()) + suffix
end function

' A day as TMDB writes it ("2026-11-04"): "Nov 4" in the year now is in,
' else "Nov 4, 2027"; "" for anything else. A day has no time zone.
function fmtDateText(text as dynamic, now as integer, zone as integer) as string
    re = CreateObject("roRegex", "^([0-9]{4})-([0-9]{2})-([0-9]{2})$", "")
    groups = re.Match(httpString(text))
    if groups.Count() < 4 then return ""
    year = groups[1].ToInt()
    month = groups[2].ToInt()
    day = groups[3].ToInt()
    if month < 1 or month > 12 or day < 1 or day > 31 then return ""
    out = fmtMonths()[month - 1] + " " + urlValue(day)
    if year <> fmtYear(now, zone) then out = out + ", " + urlValue(year)
    return out
end function
