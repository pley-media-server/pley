' Times, progress, sizes and rates as the screens write them, in the
' language shown (I18n.brs; the words in locales/<language>/format.json).
' Durations and positions are milliseconds, as the server sends them.

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
    if hours = 0 then return t("format.duration.minutes", { minutes: minutes })
    if minutes = 0 then return t("format.duration.hours", { hours: hours })
    return t("format.duration.hoursMinutes", { hours: hours, minutes: minutes })
end function

' "42 min left"; "" without a duration.
function fmtLeft(positionMs as dynamic, durationMs as dynamic) as string
    remaining = httpNumber(durationMs, 0) - httpNumber(positionMs, 0)
    if httpNumber(durationMs, 0) <= 0 or remaining <= 0 then return ""
    return t("format.duration.left", { time: fmtDuration(remaining) })
end function

' How far along, from 0 to 1.
function fmtProgress(positionMs as dynamic, durationMs as dynamic) as float
    duration = httpNumber(durationMs, 0)
    position = httpNumber(positionMs, 0)
    if duration <= 0 or position <= 0 then return 0
    if position >= duration then return 1
    return position / duration
end function

' "S1 E2" ("S1 É2", "T1 E2"); "E2" without a season; "" without an
' episode.
function fmtEpisode(season as dynamic, episode as dynamic) as string
    s = httpNumber(season, -1)
    e = httpNumber(episode, -1)
    if e < 0 then return ""
    if s < 0 then return t("format.episode.episode", { episode: Int(e) })
    return t("format.episode.both", { season: Int(s), episode: Int(e) })
end function

' "Resume from 12:34".
function fmtResume(ms as dynamic) as string
    return t("format.resume", { time: fmtClock(ms) })
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
    noun = "titles"
    if kind = "movies" then noun = "movies"
    if kind = "shows" then noun = "shows"
    if kind = "music" then noun = "artists"
    return t("format.counts." + noun, { count: n })
end function

' 7.5, 8 (no ".0"); "7,5" in French and Spanish.
function fmtOneDecimal(value as dynamic) as string
    tenths = Int(value * 10 + 0.5)
    if tenths mod 10 = 0 then return i18nNumber(Int(tenths / 10))
    return i18nNumber(tenths / 10, 1)
end function

' "8.2 Mbps", "24 Mbps", "640 kbps"; "" without one.
function fmtBitrate(bps as dynamic) as string
    value = httpNumber(bps, 0)
    if value <= 0 then return ""
    if value >= 10000000 then return t("format.bitrate.mbps", { n: i18nNumber(Int(value / 1000000 + 0.5)) })
    if value >= 1000000 then return t("format.bitrate.mbps", { n: fmtOneDecimal(value / 1000000) })
    return t("format.bitrate.kbps", { n: i18nNumber(Int(value / 1000 + 0.5)) })
end function

' 1536 → "1.5 KB", 150 MiB → "150 MB", "62.0 GB": three digits, in units
' of 1024, as the other apps write sizes. A size past 32 bits comes as a
' LongInteger, which the first division makes a Float.
function fmtBytes(bytes as dynamic) as string
    size = httpNumber(bytes, -1)
    if size < 0 then return ""
    units = ["B", "KB", "MB", "GB", "TB"]
    at = 0
    while size >= 1024 and at < units.Count() - 1
        size = size / 1024
        at = at + 1
    end while
    if size >= 100 or at = 0 then return t("format.bytes." + units[at], { n: i18nNumber(Int(size + 0.5)) })
    return t("format.bytes." + units[at], { n: i18nNumber(size, 1) })
end function

' The languages offered for the audio and subtitle defaults, as ISO 639-2/B
' codes (the other apps' 34), named in the language shown, in its
' alphabetical order: [[code, name]].
function fmtLanguages() as object
    codes = ["ara", "bul", "cat", "chi", "hrv", "cze", "dan", "dut", "eng", "fin", "fre", "ger", "gre", "heb", "hin", "hun", "ind",
        "ita", "jpn", "kor", "nor", "per", "pol", "por", "rum", "rus", "srp", "slo", "spa", "swe", "tha", "tur", "ukr", "vie"]
    named = []
    for each code in codes
        name = t("format.languages." + code)
        named.Push({ code: code, name: name, order: fmtSortKey(name) })
    end for
    named.SortBy("order")
    out = []
    for each language in named
        out.Push([language.code, language.name])
    end for
    return out
end function

' A name as it sorts: in lower case, its letters without their accents, as
' the other apps' localeCompare has them (Spanish's "Árabe" among the A's,
' French's "Hébreu" before "Hindi"). Ñ stays a letter of its own, as in
' Spanish. Capitals are mapped too, as LCase may leave them.
function fmtSortKey(name as string) as string
    plain = {
        "á": "a", "à": "a", "â": "a", "ä": "a", "Á": "a", "À": "a", "Â": "a",
        "é": "e", "è": "e", "ê": "e", "ë": "e", "É": "e", "È": "e", "Ê": "e",
        "í": "i", "î": "i", "ï": "i", "Í": "i", "Î": "i",
        "ó": "o", "ô": "o", "ö": "o", "Ó": "o", "Ô": "o",
        "ú": "u", "ù": "u", "û": "u", "ü": "u", "Ú": "u",
        "ç": "c", "Ç": "c"
    }
    key = ""
    for each letter in LCase(name).Split("")
        if plain.DoesExist(letter) then letter = plain[letter]
        key = key + letter
    end for
    return key
end function

' A language's name, its code for one not offered, none for "". Files name
' some by their ISO 639-2/T code (fra for fre).
function fmtLanguageName(code as string, none as string) as string
    if code = "" then return none
    wanted = LCase(code)
    other = { fra: "fre", deu: "ger", zho: "chi", ces: "cze", nld: "dut", ell: "gre", fas: "per", ron: "rum", slk: "slo" }
    if other.DoesExist(wanted) then wanted = other[wanted]
    key = "format.languages." + wanted
    name = t(key)
    if name = key then return code
    return name
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

' The dynamic ranges, best first; "hdr" is any of them but SDR. HDR10 and
' HDR10+ read as plain "HDR" and "HDR+", as on the posters.
function fmtDynamicRanges() as object
    return [["hdr", t("format.ranges.anyHdr")],["dolby_vision", "Dolby Vision"], ["hdr10plus", "HDR+"], ["hdr10", "HDR"], ["hlg", "HLG"], ["sdr", "SDR"]]
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
' Roku has no relative-time formatter: these are the app's own, in the
' language shown. Times are unix seconds; zone is what fmtZoneSeconds
' gives.

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
    if diff < 60 then return t("format.time.inMoment")
    minutes = Int((diff + 30) / 60)
    if minutes < 60 then return t("format.time.inMinutes", { n: minutes })
    hours = Int((diff + 1800) / 3600)
    if hours < 24 then return t("format.time.inHours", { count: hours })
    days = Int((diff + 43200) / 86400)
    if days <= 1 then return t("format.time.tomorrow")
    return t("format.time.inDays", { count: days })
end function

' How long ago at was: "now", "12 min ago", "1 hour ago", "3 hours ago",
' "yesterday", "3 days ago", else its day ("Mar 4, 2026").
function fmtAgo(at as dynamic, now as integer, zone as integer) as string
    seconds = httpNumber(at, 0)
    diff = now - seconds
    if diff < 60 then return t("format.time.now")
    if diff < 3600 then return t("format.time.minutesAgo", { n: Int(diff / 60) })
    if diff < 86400 then return t("format.time.hoursAgo", { count: Int(diff / 3600) })
    days = Int(diff / 86400)
    if days = 1 then return t("format.time.yesterday")
    if days < 30 then return t("format.time.daysAgo", { count: days })
    return fmtMonthDay(seconds, zone, true)
end function

' A month's short name (1 to 12): "Nov", "nov.", "nov".
function fmtMonth(month as integer) as string
    if month < 1 or month > 12 then return ""
    keys = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
    return t("format.date.months." + keys[month - 1])
end function

' A day as the language shown writes it: "Nov 4", "4 nov."; with its
' year, "Nov 4, 2027", "4 nov. 2027".
function fmtDayText(year as integer, month as integer, day as integer, withYear as boolean) as string
    if withYear then return t("format.date.monthDayYear", { month: fmtMonth(month), day: day, year: year })
    return t("format.date.monthDay", { month: fmtMonth(month), day: day })
end function

' at's day in local time: "Nov 4", with its year ("Nov 4, 2027") when
' withYear.
function fmtMonthDay(at as dynamic, zone as integer, withYear as boolean) as string
    date = CreateObject("roDateTime")
    date.FromSeconds(Int(httpNumber(at, 0) + zone))
    return fmtDayText(date.GetYear(), date.GetMonth(), date.GetDayOfMonth(), withYear)
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

' at's time of day in local time: "9:05 PM", "21:05".
function fmtTimeOfDay(at as dynamic, zone as integer) as string
    date = CreateObject("roDateTime")
    date.FromSeconds(Int(httpNumber(at, 0) + zone))
    return fmtClockText(date.GetHours(), date.GetMinutes(), fmtTwelveHour())
end function

' A time of day: "9:05 PM" on a 12-hour clock, else "21:05" ("9:05" in
' Spanish, as CLDR writes it).
function fmtClockText(hours as integer, minutes as integer, twelve as boolean) as string
    if not twelve
        if i18nLanguage() = "es" then return hours.ToStr() + ":" + fmtTwo(minutes)
        return fmtTwo(hours) + ":" + fmtTwo(minutes)
    end if
    shown = hours mod 12
    if shown = 0 then shown = 12
    time = shown.ToStr() + ":" + fmtTwo(minutes)
    if hours >= 12 then return t("format.clock.pm", { time: time })
    return t("format.clock.am", { time: time })
end function

' Whether times show on a 12-hour clock: in English, as the Roku's clock
' is set (Settings → System → Time); French and Spanish write 24 hours.
function fmtTwelveHour() as boolean
    if i18nLanguage() <> "en" then return false
    g = GetGlobalAA()
    if g.fmtClockFormat = invalid then g.fmtClockFormat = CreateObject("roDeviceInfo").GetClockFormat()
    return g.fmtClockFormat <> "24h"
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
    return fmtDayText(year, month, day, year <> fmtYear(now, zone))
end function
