' The languages the app is in: English, French and Spanish, and the words
' of each, looked up with t().
'
' The words are in locales/<language>/<area>.json, in the format of the
' other apps' catalogs (i18next's: nested keys, {{name}} for values,
' _one, _many and _other for counts). A key is its area and its path:
' t("signin.servers.title") reads servers.title in locales/<language>/
' signin.json. Roku's own tr() follows the device's language only, so it
' can't show the account's; these are read with ParseJson, an area at a
' time, the first time a component (or a task) asks for one of its words,
' and kept in its m until the language changes. A word a language lacks
' shows in English.
'
' The language shown is m.global.language, which the scene sets
' (MainScene.brs); a thread that sets its own with i18nUse (the API task
' for each request, the tests) reads that instead.

function i18nLanguages() as object
    return ["en", "fr", "es"]
end function

' The language Pley has for a tag ("fr-CA", "FR_ca", "fr" → "fr"); "" for
' none.
function i18nSupported(tag as dynamic) as string
    if GetInterface(tag, "ifString") = invalid then return ""
    base = LCase(tag.Trim())
    for each separator in ["-", "_"]
        at = Instr(1, base, separator)
        if at > 0 then base = Left(base, at - 1)
    end for
    for each language in i18nLanguages()
        if language = base then return language
    end for
    return ""
end function

' The first of the device's languages (tags, its preferred first) that
' Pley has; "" for none.
function i18nDeviceLanguage(tags as dynamic) as string
    if GetInterface(tags, "ifArray") = invalid then return i18nSupported(tags)
    for each tag in tags
        language = i18nSupported(tag)
        if language <> "" then return language
    end for
    return ""
end function

' The language to show, as on the other TVs: the account's, else the one
' picked on this Roku (choice, with the globe before signing in), else the
' Roku's own when Pley has it, else the server's, else English. Anything
' Pley isn't in counts as none (packages/tv/test/fixtures/language.json).
function i18nPick(account as dynamic, server as dynamic, device as dynamic, choice as dynamic) as string
    language = i18nSupported(account)
    if language = "" then language = i18nSupported(choice)
    if language = "" then language = i18nSystem(server, device)
    return language
end function

' What Automatic is with no choice: the Roku's language when Pley has it,
' else the server's, else English.
function i18nSystem(server as dynamic, device as dynamic) as string
    language = i18nDeviceLanguage(device)
    if language = "" then language = i18nSupported(server)
    if language = "" then language = "en"
    return language
end function

' A language's name in itself: "English", "Français", "Español".
function i18nName(language as string) as string
    names = { en: "English", fr: "Français", es: "Español" }
    name = names[language]
    if name = invalid then return language
    return name
end function

' The language the words are shown in now.
function i18nLanguage() as string
    g = GetGlobalAA()
    fixed = g.i18nFixed
    if GetInterface(fixed, "ifString") <> invalid and fixed <> "" then return fixed
    if GetInterface(g.global, "ifSGNodeField") <> invalid
        shown = g.global.language
        if GetInterface(shown, "ifString") <> invalid and shown <> "" then return shown
    end if
    return "en"
end function

' This thread shows its words in language from now on (the API task, for
' each request; the tests); "" goes back to the scene's.
sub i18nUse(language as string)
    GetGlobalAA().i18nFixed = i18nSupported(language)
end sub

' --- Counting ---

' The plural form of n in language, as CLDR has it: English "one" is
' exactly 1; French "one" is 0 to 1.99…; Spanish "one" is 1. French and
' Spanish say "many" for a whole number of millions (1 000 000, 2 000 000…).
function i18nPluralForm(language as string, n as dynamic) as string
    value = i18nNumeric(n)
    if value < 0 then value = -value
    whole = Int(value)
    if language = "en"
        if value = 1 then return "one"
        return "other"
    end if
    if language = "fr"
        if whole = 0 or whole = 1 then return "one"
    else if value = 1
        return "one"
    end if
    if whole = value and whole <> 0 and whole mod 1000000 = 0 then return "many"
    return "other"
end function

' --- Numbers ---

' n as the language shown writes it: "12,345" in English, "12 345" in
' French (a narrow no-break space), "12.345" in Spanish, which groups from
' five digits ("1234"); decimals, when given, after "." or ",".
function i18nNumber(n as dynamic, decimals = 0 as integer) as string
    value = i18nNumeric(n)
    language = i18nLanguage()
    negative = value < 0
    if negative then value = -value
    scale = 1
    for i = 1 to decimals
        scale = scale * 10
    end for
    ' A whole number stays one: a Float keeps only 7 digits.
    if decimals = 0 and (GetInterface(value, "ifInt") <> invalid or GetInterface(value, "ifLongInt") <> invalid)
        rounded = value
    else
        rounded = Int(value * scale + 0.5)
    end if
    whole = rounded \ scale
    digits = i18nDigits(whole)
    group = ","
    point = "."
    if language = "fr"
        group = Chr(8239)
        point = ","
    else if language = "es"
        group = "."
        point = ","
    end if
    if Len(digits) > 4 or (Len(digits) = 4 and language <> "es")
        out = ""
        while Len(digits) > 3
            out = group + Right(digits, 3) + out
            digits = Left(digits, Len(digits) - 3)
        end while
        digits = digits + out
    end if
    if decimals > 0
        fraction = i18nDigits(rounded - whole * scale)
        while Len(fraction) < decimals
            fraction = "0" + fraction
        end while
        digits = digits + point + fraction
    end if
    if negative and rounded > 0 then digits = "-" + digits
    return digits
end function

' n when it is a number, else 0.
function i18nNumeric(n as dynamic) as dynamic
    for each kind in ["ifInt", "ifLongInt", "ifFloat", "ifDouble"]
        if GetInterface(n, kind) <> invalid then return n
    end for
    return 0
end function

' A whole number's digits ("12345").
function i18nDigits(whole as dynamic) as string
    if GetInterface(whole, "ifLongInt") <> invalid or GetInterface(whole, "ifInt") <> invalid then return whole.ToStr()
    if whole < 2147483647 and whole > -2147483647 then return Int(whole).ToStr()
    return Str(whole).Trim()
end function

' --- Words ---

' The words of key ("signin.servers.title") in the language shown, with
' values ({ name: "Anna" }) in their {{names}}; with a count among them,
' the plural form it takes. English stands in for words a language lacks;
' the key itself shows when no language has them.
function t(key as string, values = invalid as dynamic) as string
    shown = i18nLanguage()
    g = GetGlobalAA()
    if g.i18nCache = invalid or g.i18nCache.language <> shown then g.i18nCache = { language: shown, areas: {} }
    count = invalid
    if GetInterface(values, "ifAssociativeArray") <> invalid and values.DoesExist("count") then count = values.count
    text = i18nTemplate(shown, key, count)
    if text = invalid and shown <> "en" then text = i18nTemplate("en", key, count)
    if text = invalid then return key
    if values = invalid or Instr(1, text, "{{") = 0 then return text
    return i18nFill(text, values)
end function

' The words of key in language, before their values: the plural form
' count takes when it counts; invalid when language lacks them.
function i18nTemplate(language as string, key as string, count as dynamic) as dynamic
    parts = key.Split(".")
    last = parts.Count() - 1
    if last < 1 then return invalid
    at = i18nArea(language, parts[0])
    for i = 1 to last - 1
        at = at[parts[i]]
        if GetInterface(at, "ifAssociativeArray") = invalid then return invalid
    end for
    leaf = parts[last]
    if count <> invalid
        text = at[leaf + "_" + i18nPluralForm(language, count)]
        if GetInterface(text, "ifString") = invalid then text = at[leaf + "_other"]
        if GetInterface(text, "ifString") <> invalid then return text
    end if
    text = at[leaf]
    if GetInterface(text, "ifString") = invalid then return invalid
    return text
end function

' An area's words in language ({} when it has no such file), read once
' for this component (or thread) until the language shown changes.
function i18nArea(language as string, area as string) as object
    g = GetGlobalAA()
    if g.i18nCache = invalid then g.i18nCache = { language: i18nLanguage(), areas: {} }
    areas = g.i18nCache.areas
    key = language + "/" + area
    words = areas[key]
    if words = invalid
        words = ParseJson(ReadAsciiFile("pkg:/locales/" + key + ".json"))
        if GetInterface(words, "ifAssociativeArray") = invalid then words = {}
        areas[key] = words
    end if
    return words
end function

' text with its {{names}} written in from values, {{name, number}} as the
' language shown writes numbers. A value is written as it is: what it
' holds is never read as a name.
function i18nFill(text as string, values as object) as string
    out = ""
    rest = text
    while true
        openAt = Instr(1, rest, "{{")
        if openAt = 0 then exit while
        closeAt = Instr(openAt + 2, rest, "}}")
        if closeAt = 0 then exit while
        inside = Mid(rest, openAt + 2, closeAt - openAt - 2)
        name = inside.Trim()
        style = ""
        comma = Instr(1, name, ",")
        if comma > 0
            style = Mid(name, comma + 1).Trim()
            name = Left(name, comma - 1).Trim()
        end if
        value = values.Lookup(name)
        if value = invalid
            written = "{{" + inside + "}}"
        else if style = "number" and GetInterface(value, "ifString") = invalid
            written = i18nNumber(value)
        else
            written = i18nText(value)
        end if
        out = out + Left(rest, openAt - 1) + written
        rest = Mid(rest, closeAt + 2)
    end while
    return out + rest
end function

' A value as text: strings as they are, numbers without Str()'s space.
function i18nText(value as dynamic) as string
    if GetInterface(value, "ifString") <> invalid then return value
    if GetInterface(value, "ifInt") <> invalid or GetInterface(value, "ifLongInt") <> invalid then return value.ToStr()
    if GetInterface(value, "ifFloat") <> invalid or GetInterface(value, "ifDouble") <> invalid
        if value = Int(value) then return i18nDigits(value)
        return Str(value).Trim()
    end if
    if GetInterface(value, "ifBoolean") <> invalid
        if value then return "true"
        return "false"
    end if
    return ""
end function
