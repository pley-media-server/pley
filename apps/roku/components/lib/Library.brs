' A library's page as the web app has it (apps/web/src/pages/LibraryPage.tsx):
' its tabs (Recommended, Library, and Collections for movies and shows),
' and the Library tab's sort and filters. Which tab, sort and filters a
' library was left on is kept on the Roku for each server, profile and
' library, and it opens on them again.

' The tabs of a library of kind, in order.
function libTabs(kind as string) as object
    if kind = "music" then return ["recommended", "library"]
    return ["recommended", "library", "collections"]
end function

function libTabLabel(which as string) as string
    if which = "recommended" then return "Recommended"
    if which = "collections" then return "Collections"
    return "Library"
end function

' The sorts a library of kind offers, in order: [{ id, label }].
function libSorts(kind as string) as object
    all = [
        { id: "title", label: "Title" }, { id: "year", label: "Year" }, { id: "added", label: "Date added" },
        { id: "rating", label: "Rating" }, { id: "user_rating", label: "Your rating" }, { id: "release", label: "Release date" },
        { id: "duration", label: "Duration" }, { id: "last_played", label: "Last played" }, { id: "bitrate", label: "Bitrate" }
    ]
    if kind = "music" then return [all[0], all[2], all[4], all[7]]
    if kind = "shows"
        out = []
        for each sort in all
            if sort.id <> "duration" then out.Push(sort)
        end for
        return out
    end if
    return all
end function

' A sort's own order: A to Z for titles, newest or highest first for the rest.
function libDefaultOrder(sort as string) as string
    if sort = "title" then return "asc"
    return "desc"
end function

' How the Library tab is sorted, in words: "Title, A to Z", "Year, newest
' first".
function libSortLabel(sort as string, order as string) as string
    notes = {
        title: ["A to Z", "Z to A"],
        year: ["oldest first", "newest first"],
        added: ["first added first", "recently added first"],
        rating: ["lowest rated first", "best rated first"],
        user_rating: ["lowest rated first", "best rated first"],
        release: ["earliest first", "latest first"],
        duration: ["shortest first", "longest first"],
        last_played: ["least recent first", "most recent first"],
        bitrate: ["lowest first", "highest first"]
    }
    label = "Title"
    for each known in libSorts("movies")
        if known.id = sort then label = known.label
    end for
    pair = notes[sort]
    if pair = invalid then pair = notes.title
    if order = "desc" then return label + ", " + pair[1]
    return label + ", " + pair[0]
end function

' The view of a library a fresh Roku opens on: the Library tab by title,
' nothing filtered. watched: "" for every title, "false" unwatched only,
' "true" watched only; genre "" and year 0 for any; resolution and
' dynamic_range a token of fmtResolutions and fmtDynamicRanges, "" for any.
function libDefaultView() as object
    return { tab: "library", sort: "title", order: "asc", watched: "", genre: "", year: 0, resolution: "", dynamic_range: "" }
end function

' The view a row's See all opens a library on: the Library tab in the
' row's order (newest or best first), nothing filtered; a sort the library
' doesn't offer is Title. It isn't kept until the person changes it.
function libSeeAllView(sort as string, kind as string) as object
    view = libDefaultView()
    for each known in libSorts(kind)
        if known.id = sort then view.sort = sort
    end for
    view.order = libDefaultOrder(view.sort)
    return view
end function

' The query of the Library tab's page at offset.
function libQuery(view as object, offset as integer, limit as integer) as object
    query = { sort: view.sort, order: view.order, offset: offset, limit: limit }
    if view.watched <> "" then query.watched = view.watched
    if view.genre <> "" then query.genre = view.genre
    if view.year > 0 then query.year = view.year
    if view.resolution <> "" then query.resolution = view.resolution
    if view.dynamic_range <> "" then query.dynamic_range = view.dynamic_range
    return query
end function

' What the filters keep, in words: "All titles", "Unwatched, Comedy, 1999,
' 4K, Dolby Vision".
function libFilterLabel(view as object) as string
    parts = []
    if view.watched = "false" then parts.Push("Unwatched")
    if view.watched = "true" then parts.Push("Watched")
    if view.genre <> "" then parts.Push(view.genre)
    if view.year > 0 then parts.Push(urlValue(view.year))
    if view.resolution <> "" then parts.Push(fmtTokenLabel(fmtResolutions(), view.resolution))
    if view.dynamic_range <> "" then parts.Push(fmtTokenLabel(fmtDynamicRanges(), view.dynamic_range))
    if parts.Count() = 0 then return "All titles"
    return parts.Join(", ")
end function

function libFiltered(view as object) as boolean
    return view.watched <> "" or view.genre <> "" or view.year > 0 or view.resolution <> "" or view.dynamic_range <> ""
end function

' Clear filters: every title again, in the same order.
sub libClearFilters(view as object)
    view.watched = ""
    view.genre = ""
    view.year = 0
    view.resolution = ""
    view.dynamic_range = ""
end sub

' --- The Filter panel ---

' What GET /api/libraries/{id}/filters offers: { genres, years,
' resolutions, dynamicRanges }. The resolutions and dynamic ranges are
' tokens, in the server's order, and only those this version can name; an
' older server sends neither.
function libFilters(data as dynamic) as object
    answer = httpObject(data)
    if answer = invalid then answer = {}
    filters = { genres: [], years: [], resolutions: libKnownTokens(answer.resolutions, fmtResolutions()), dynamicRanges: libKnownTokens(answer.dynamic_ranges, fmtDynamicRanges()) }
    for each genre in httpArray(answer.genres)
        name = httpSentence(genre)
        if name <> "" then filters.genres.Push(name)
    end for
    for each year in httpArray(answer.years)
        if httpNumber(year, 0) > 0 then filters.years.Push(Int(year))
    end for
    return filters
end function

function libKnownTokens(list as dynamic, table as object) as object
    tokens = []
    for each token in httpArray(list)
        if fmtTokenLabel(table, token) <> "" then tokens.Push(token)
    end for
    return tokens
end function

' The Filter panel's rows, in order: [{ title, action }]. Show, Genre and
' Year; Resolution and Dynamic range when the library's titles differ in
' them (filters: libFilters, invalid until the server said) or one is
' chosen, so it can be changed or cleared, never for music; then Clear
' filters and Save as… once some are set.
function libFilterRows(view as object, kind as string, filters as dynamic) as object
    rows = [{ title: "Show: " + libWatchedLabel(view.watched), action: "watched" }]
    genre = "Any"
    if view.genre <> "" then genre = view.genre
    rows.Push({ title: "Genre: " + genre, action: "genre" })
    year = "Any"
    if view.year > 0 then year = urlValue(view.year)
    rows.Push({ title: "Year: " + year, action: "year" })
    if kind <> "music"
        resolutions = []
        ranges = []
        if filters <> invalid
            resolutions = filters.resolutions
            ranges = filters.dynamicRanges
        end if
        if resolutions.Count() > 0 or view.resolution <> ""
            rows.Push({ title: "Resolution: " + libQualityLabel(fmtResolutions(), view.resolution), action: "resolution" })
        end if
        if ranges.Count() > 0 or view.dynamic_range <> ""
            rows.Push({ title: "Dynamic range: " + libQualityLabel(fmtDynamicRanges(), view.dynamic_range), action: "dynamic_range" })
        end if
    end if
    if libFiltered(view)
        rows.Push({ title: "Clear filters", action: "clear" })
        rows.Push({ title: "Save as…", action: "save" })
    end if
    return rows
end function

function libWatchedLabel(watched as string) as string
    if watched = "false" then return "Unwatched"
    if watched = "true" then return "Watched"
    return "All titles"
end function

function libQualityLabel(table as object, token as string) as string
    if token = "" then return "Any"
    return fmtTokenLabel(table, token)
end function

' A resolution's or a dynamic range's choices: { labels, values, current }.
' Any, then the tokens the library offers in its order, and the one chosen
' when it no longer offers it, so that it shows checked.
function libQualityChoices(table as object, offered as object, chosen as string) as object
    out = { labels: ["Any"], values: [""], current: 0 }
    tokens = []
    tokens.Append(offered)
    if chosen <> ""
        known = false
        for each token in tokens
            if token = chosen then known = true
        end for
        if not known then tokens.Push(chosen)
    end if
    for each token in tokens
        if token = chosen then out.current = out.values.Count()
        out.labels.Push(fmtTokenLabel(table, token))
        out.values.Push(token)
    end for
    return out
end function

' --- Save as… ---

' Plex's name of each sort, as smart filters write it (docs/COMPATIBILITY.md).
function libPlexSort(sort as string) as string
    names = {
        title: "titleSort", year: "year", added: "addedAt", rating: "rating", user_rating: "userRating",
        release: "originallyAvailableAt", duration: "duration", last_played: "lastViewedAt", bitrate: "mediaBitrate"
    }
    if names.DoesExist(sort) then return names[sort]
    return "titleSort"
end function

' The Library tab's filters and order as a smart filter, in Plex's syntax
' (genre=Comedy&unwatched=1&sort=year:desc, docs/API.md's smart playlists;
' dynamicRange is Pley's own field): what Save as… saves, written as the
' other apps write it (Android's SmartFilter.of).
function libSmartFilter(view as object) as string
    parts = []
    if view.genre <> "" then parts.Push("genre=" + libFormValue(view.genre))
    if view.year > 0 then parts.Push("year=" + urlValue(view.year))
    if view.resolution <> "" then parts.Push("resolution=" + libFormValue(view.resolution))
    if view.dynamic_range <> "" then parts.Push("dynamicRange=" + libFormValue(view.dynamic_range))
    if view.watched = "true" then parts.Push("unwatched=0")
    if view.watched = "false" then parts.Push("unwatched=1")
    field = libPlexSort(view.sort)
    if view.order = "desc" then field = field + ":desc"
    parts.Push("sort=" + libFormValue(field))
    return parts.Join("&")
end function

' A value of a form, spaces as "+" (as URLSearchParams and Java write it).
function libFormValue(text as string) as string
    return urlEncode(text).Replace("%20", "+")
end function

' What libSmartFilter's filter finds, in words, as the other apps describe
' a smart playlist: ["Genre: Comedy", "4K", "Unwatched", "Newest first"].
function libSmartFilterWords(view as object) as object
    words = []
    if view.genre <> "" then words.Push("Genre: " + view.genre)
    if view.year > 0 then words.Push("From " + urlValue(view.year))
    if view.resolution <> "" then words.Push(fmtTokenLabel(fmtResolutions(), view.resolution))
    if view.dynamic_range <> "" then words.Push(fmtTokenLabel(fmtDynamicRanges(), view.dynamic_range))
    if view.watched = "true" then words.Push("Watched")
    if view.watched = "false" then words.Push("Unwatched")
    notes = {
        titleSort: ["By title", "By title, Z to A"],
        year: ["Oldest first", "Newest first"],
        originallyAvailableAt: ["Earliest release first", "Latest release first"],
        addedAt: ["First added first", "Recently added first"],
        rating: ["Lowest rated first", "Best rated first"],
        userRating: ["Your lowest rated first", "Your best rated first"],
        lastViewedAt: ["Least recently played first", "Recently played first"],
        duration: ["Shortest first", "Longest first"],
        mediaBitrate: ["Lowest bitrate first", "Highest bitrate first"]
    }
    pair = notes[libPlexSort(view.sort)]
    if view.order = "desc"
        words.Push(pair[1])
    else
        words.Push(pair[0])
    end if
    return words
end function

' Where a library's view is kept: the server, the profile and the library.
function libViewKey(server as string, profileId as dynamic, libraryId as dynamic) as string
    return server + "|" + urlValue(profileId) + "|" + urlValue(libraryId)
end function

' The view kept for key, checked against what a library of kind offers;
' the default for anything else. Music has no resolutions or dynamic
' ranges to filter by.
function libLoadView(section as object, key as string, kind as string) as object
    view = libDefaultView()
    kept = httpObject(httpJson(section.Read(key)))
    if kept = invalid then return view
    shownTab = httpString(kept.tab)
    for each known in libTabs(kind)
        if known = shownTab then view.tab = shownTab
    end for
    sort = httpString(kept.sort)
    for each known in libSorts(kind)
        if known.id = sort then view.sort = sort
    end for
    view.order = libDefaultOrder(view.sort)
    order = httpString(kept.order)
    if order = "asc" or order = "desc" then view.order = order
    watched = httpString(kept.watched)
    if watched = "true" or watched = "false" then view.watched = watched
    view.genre = httpSentence(kept.genre)
    year = httpNumber(kept.year, 0)
    if year > 0 then view.year = Int(year)
    if kind <> "music"
        if fmtTokenLabel(fmtResolutions(), kept.resolution) <> "" then view.resolution = kept.resolution
        if fmtTokenLabel(fmtDynamicRanges(), kept.dynamic_range) <> "" then view.dynamic_range = kept.dynamic_range
    end if
    return view
end function

sub libSaveView(section as object, key as string, view as object)
    section.Write(key, FormatJson(view))
    section.Flush()
end sub

' --- A to Z ---

' The Library tab sorted by title jumps to a letter, as on the other apps
' (Android's AlphaIndex). The server has no index of letters: the app reads
' the listing's titles, a page at a time, and keeps only where each
' letter's titles start.

' The letters of the jump: "#", then A to Z.
function libLetters() as object
    return ["#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"]
end function

' Whether the Library tab offers the jump: sorted by title (either way),
' with more than 30 titles.
function libShowsJump(view as object, total as integer) as boolean
    return view.sort = "title" and total > 30
end function

' The letter a title goes under: 1 to 26 for A to Z, accents taken off
' ("Élan" under E); 0 ("#") for anything else, digits and other alphabets.
function libLetterOf(title as string) as integer
    first = Left(title.Trim(), 1)
    if first = "" then return 0
    code = Asc(UCase(first))
    if code >= 65 and code <= 90 then return code - 64
    plain = libAccented()[first]
    if plain = invalid then return 0
    return plain
end function

' The Latin letters with an accent that comes off (Unicode's decomposition,
' as the other apps take it off), each to its letter's number; made once
' per thread. Letters of their own (Ø, Ł, Æ) aren't among them.
function libAccented() as object
    globals = GetGlobalAA()
    if globals.libAccented = invalid
        accented = {}
        groups = {
            A: "ÀÁÂÃÄÅàáâãäåĀāĂăĄąǍǎ",
            C: "ÇçĆćĈĉĊċČč",
            D: "Ďď",
            E: "ÈÉÊËèéêëĒēĔĕĖėĘęĚě",
            G: "ĜĝĞğĠġĢģ",
            H: "Ĥĥ",
            I: "ÌÍÎÏìíîïĨĩĪīĬĭĮįİǏǐ",
            J: "Ĵĵ",
            K: "Ķķ",
            L: "ĹĺĻļĽľ",
            N: "ÑñŃńŅņŇň",
            O: "ÒÓÔÕÖòóôõöŌōŎŏŐőǑǒ",
            R: "ŔŕŖŗŘř",
            S: "ŚśŜŝŞşŠš",
            T: "ŢţŤť",
            U: "ÙÚÛÜùúûüŨũŪūŬŭŮůŰűŲųǓǔ",
            W: "Ŵŵ",
            Y: "ÝýÿŶŷŸ",
            Z: "ŹźŻżŽž"
        }
        for each letter in groups
            number = Asc(UCase(letter)) - 64
            characters = groups[letter]
            for i = 1 to Len(characters)
                accented[Mid(characters, i, 1)] = number
            end for
        end for
        globals.libAccented = accented
    end if
    return globals.libAccented
end function

' The letters of a page of a listing (ItemSummary[]: the sort title, else
' the title) as runs, one where the letter changes: [[letter, position in
' the page]].
function libLetterRuns(items as dynamic) as object
    runs = []
    previous = -1
    position = 0
    for each item in httpArray(items)
        title = ""
        if GetInterface(item, "ifAssociativeArray") <> invalid
            title = httpString(item.sort_title)
            if title.Trim() = "" then title = httpString(item.title)
        end if
        letter = libLetterOf(title)
        if letter <> previous then runs.Push([letter, position])
        previous = letter
        position = position + 1
    end for
    return runs
end function

' A listing's A to Z, made a page at a time (libAlphaAdd): { firsts (each
' letter's first title, -1 for none), starts and letters (the runs, in
' order), count (the titles so far) }.
function libAlphaNew() as object
    firsts = []
    for i = 0 to 26
        firsts.Push(-1)
    end for
    return { firsts: firsts, starts: [], letters: [], count: 0 }
end function

' The next page's runs (libLetterRuns) and how many titles it listed.
sub libAlphaAdd(index as object, runs as object, listed as integer)
    for each pair in runs
        letter = pair[0]
        at = index.count + pair[1]
        if index.firsts[letter] < 0 then index.firsts[letter] = at
        last = index.letters.Count() - 1
        if last < 0 or index.letters[last] <> letter
            index.starts.Push(at)
            index.letters.Push(letter)
        end if
    end for
    index.count = index.count + listed
end sub

function libAlphaHas(index as object, letter as integer) as boolean
    return index.firsts[letter] >= 0
end function

' Where letter jumps to: its first title, else the next letter's that has
' one, else the last letter's before it, else the top.
function libAlphaJump(index as object, letter as integer) as integer
    for l = letter to 26
        if index.firsts[l] >= 0 then return index.firsts[l]
    end for
    for l = letter to 0 step -1
        if index.firsts[l] >= 0 then return index.firsts[l]
    end for
    return 0
end function

' The letter of the run position is in.
function libAlphaLetterAt(index as object, position as integer) as integer
    starts = index.starts
    if starts.Count() = 0 then return 0
    lo = 0
    hi = starts.Count() - 1
    if position < starts[0] then return index.letters[0]
    while lo < hi
        middle = Int((lo + hi + 1) / 2)
        if starts[middle] <= position
            lo = middle
        else
            hi = middle - 1
        end if
    end while
    return index.letters[lo]
end function
