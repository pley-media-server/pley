' A library's tabs, sort and filters, and how they are kept.

sub test_library_tabs_by_kind()
    tkEqual(libTabs("movies"), ["recommended", "library", "collections"])
    tkEqual(libTabs("music"), ["recommended", "library"], "music has no collections")
    tkEqual(libTabLabel("recommended"), "Recommended")
    tkEqual(libTabLabel("library"), "Library")
end sub

sub test_library_sorts_by_kind()
    ids = function(sorts as object) as object
        out = []
        for each sort in sorts
            out.Push(sort.id)
        end for
        return out
    end function
    tkEqual(ids(libSorts("movies")), ["title", "year", "added", "rating", "user_rating", "release", "duration", "last_played", "bitrate"])
    tkEqual(ids(libSorts("shows")), ["title", "year", "added", "rating", "user_rating", "release", "last_played", "bitrate"], "shows: no duration")
    tkEqual(ids(libSorts("music")), ["title", "added", "user_rating", "last_played"])
    tkEqual(libDefaultOrder("title"), "asc")
    tkEqual(libDefaultOrder("added"), "desc")
    tkEqual(libSortLabel("title", "asc"), "Title, A to Z")
    tkEqual(libSortLabel("year", "desc"), "Year, newest first")
    tkEqual(libSortLabel("added", "asc"), "Date added, first added first")
end sub

sub test_library_query_and_filters()
    view = libDefaultView()
    tkEqual(libQuery(view, 0, 70), { sort: "title", order: "asc", offset: 0, limit: 70 })
    tkEqual(libFilterLabel(view), "All titles")
    tkFalse(libFiltered(view))
    view.watched = "false"
    view.genre = "Comedy"
    view.year = 1999
    tkEqual(libQuery(view, 70, 70), { sort: "title", order: "asc", offset: 70, limit: 70, watched: "false", genre: "Comedy", year: 1999 })
    tkEqual(libFilterLabel(view), "Unwatched, Comedy, 1999")
    tkTrue(libFiltered(view))
    view.resolution = "4k"
    view.dynamic_range = "dolby_vision"
    tkEqual(libQuery(view, 0, 70), { sort: "title", order: "asc", offset: 0, limit: 70, watched: "false", genre: "Comedy", year: 1999, resolution: "4k", dynamic_range: "dolby_vision" })
    tkEqual(libFilterLabel(view), "Unwatched, Comedy, 1999, 4K, Dolby Vision", "labels, never tokens")
    libClearFilters(view)
    tkEqual(view, libDefaultView(), "Clear filters clears them all")
    view.dynamic_range = "hdr"
    tkTrue(libFiltered(view), "a dynamic range alone filters")
    tkEqual(libQuery(view, 0, 70), { sort: "title", order: "asc", offset: 0, limit: 70, dynamic_range: "hdr" })
    tkEqual(libFilterLabel(view), "Any HDR")
    view.dynamic_range = ""
    view.resolution = "sd"
    tkTrue(libFiltered(view), "a resolution alone filters")
    tkEqual(libFilterLabel(view), "SD")
end sub

sub test_library_posters_badged_only_where_it_says_something()
    view = libDefaultView()
    tkFalse(libShowsBadges(view), "by title, nothing filtered")
    view.watched = "false"
    view.genre = "Drama"
    view.year = 1999
    view.sort = "added"
    tkFalse(libShowsBadges(view), "other sorts and filters")
    view.sort = "bitrate"
    tkTrue(libShowsBadges(view), "sorted by bitrate")
    view.order = "asc"
    tkTrue(libShowsBadges(view), "either way")
    view.sort = "title"
    view.resolution = "1080"
    tkTrue(libShowsBadges(view), "filtered by resolution")
    view.resolution = ""
    view.dynamic_range = "sdr"
    tkTrue(libShowsBadges(view), "filtered by dynamic range")
end sub

sub test_library_filters_the_server_offers()
    filters = libFilters(ParseJson("{""genres"":[""Drama"","""",3],""years"":[2022,0,""x""],""resolutions"":[""4k"",""1080"",""8k"",7],""dynamic_ranges"":[""hdr"",""dolby_vision"",""sdr""]}"))
    tkEqual(filters, { genres: ["Drama"], years: [2022], resolutions: ["4k", "1080"], dynamicRanges: ["hdr", "dolby_vision", "sdr"] }, "tokens this version can't name left out")
    older = libFilters(ParseJson("{""genres"":[""Drama""],""years"":[2022]}"))
    tkEqual(older.resolutions, [], "an older server sends neither")
    tkEqual(older.dynamicRanges, [])
    tkEqual(libFilters(invalid), { genres: [], years: [], resolutions: [], dynamicRanges: [] })
end sub

sub test_library_filter_panel_rows()
    actions = function(rows as object) as object
        out = []
        for each row in rows
            out.Push(row.action)
        end for
        return out
    end function
    view = libDefaultView()
    offered = { genres: ["Drama"], years: [2022], resolutions: ["4k", "1080"], dynamicRanges: ["hdr", "dolby_vision", "sdr"] }
    none = { genres: ["Drama"], years: [2022], resolutions: [], dynamicRanges: [] }
    tkEqual(actions(libFilterRows(view, "movies", invalid)), ["watched", "genre", "year"], "until the server says")
    tkEqual(actions(libFilterRows(view, "movies", none)), ["watched", "genre", "year"], "a library whose titles are all alike")
    rows = libFilterRows(view, "shows", offered)
    tkEqual(actions(rows), ["watched", "genre", "year", "resolution", "dynamic_range"])
    tkEqual(rows[0].title, "Show: All titles")
    tkEqual(rows[3].title, "Resolution: Any")
    tkEqual(rows[4].title, "Dynamic range: Any")
    tkEqual(actions(libFilterRows(view, "music", offered)), ["watched", "genre", "year"], "never for music")
    view.resolution = "4k"
    rows = libFilterRows(view, "movies", none)
    tkEqual(actions(rows), ["watched", "genre", "year", "resolution", "clear", "save"], "a chosen one stays, to be changed or cleared")
    tkEqual(rows[3].title, "Resolution: 4K")
    view.resolution = ""
    view.dynamic_range = "dolby_vision"
    view.genre = "Drama"
    rows = libFilterRows(view, "movies", offered)
    tkEqual(actions(rows), ["watched", "genre", "year", "resolution", "dynamic_range", "clear", "save"])
    tkEqual(rows[1].title, "Genre: Drama")
    tkEqual(rows[4].title, "Dynamic range: Dolby Vision")
end sub

sub test_library_quality_filter_choices()
    choices = libQualityChoices(fmtResolutions(), ["4k", "1080", "sd"], "")
    tkEqual(choices, { labels: ["Any", "4K", "1080p", "SD"], values: ["", "4k", "1080", "sd"], current: 0 }, "the server's, in its order")
    tkEqual(libQualityChoices(fmtResolutions(), ["4k", "1080", "sd"], "1080").current, 2)
    choices = libQualityChoices(fmtDynamicRanges(), ["hdr", "dolby_vision", "sdr"], "hdr10")
    tkEqual(choices.labels, ["Any", "Any HDR", "Dolby Vision", "SDR", "HDR"], "the one chosen, no longer offered, still there")
    tkEqual(choices.current, 4)
    tkEqual(libQualityChoices(fmtDynamicRanges(), [], "hlg"), { labels: ["Any", "HLG"], values: ["", "hlg"], current: 1 })
end sub

sub test_library_views_kept_and_checked()
    section = CreateObject("roRegistrySection", "libraryViewsTest")
    key = libViewKey("http://nas:7539", 4, 2)
    tkEqual(key, "http://nas:7539|4|2")
    section.Delete(key)
    tkEqual(libLoadView(section, key, "movies"), libDefaultView(), "nothing kept: the defaults")
    view = libDefaultView()
    view.tab = "collections"
    view.sort = "duration"
    view.order = "desc"
    view.genre = "Drama"
    libSaveView(section, key, view)
    tkEqual(libLoadView(section, key, "movies"), view)
    shows = libLoadView(section, key, "shows")
    tkEqual(shows.sort, "title", "a sort the kind doesn't offer")
    tkEqual(shows.order, "desc", "the order kept")
    music = libLoadView(section, key, "music")
    tkEqual(music.tab, "library", "a tab the kind doesn't have")
    view.resolution = "4k"
    view.dynamic_range = "hdr10plus"
    libSaveView(section, key, view)
    tkEqual(libLoadView(section, key, "movies"), view, "a resolution and a dynamic range kept")
    music = libLoadView(section, key, "music")
    tkEqual(music.resolution, "", "music has neither")
    tkEqual(music.dynamic_range, "")
    tkEqual(music.genre, "Drama")
    section.Write(key, "{""sort"":""title"",""resolution"":""4K"",""dynamic_range"":""hdr11""}")
    kept = libLoadView(section, key, "movies")
    tkEqual(kept.resolution, "", "not a token: dropped")
    tkEqual(kept.dynamic_range, "")
    section.Write(key, "{""resolution"":1080,""dynamic_range"":[""hdr""]}")
    tkEqual(libLoadView(section, key, "movies"), libDefaultView(), "not even text")
    section.Write(key, "not json")
    tkEqual(libLoadView(section, key, "movies"), libDefaultView())
    section.Delete(key)
end sub

sub test_library_see_all_views()
    view = libSeeAllView("added", "movies")
    tkEqual(view, { tab: "library", sort: "added", order: "desc", watched: "", genre: "", year: 0, resolution: "", dynamic_range: "" }, "newest first, nothing filtered")
    tkEqual(libSeeAllView("rating", "shows").order, "desc")
    tkEqual(libSeeAllView("release", "music").sort, "title", "a sort music doesn't offer")
    tkEqual(libSeeAllView("release", "music").order, "asc")
end sub

sub test_library_filters_as_a_smart_filter()
    view = libDefaultView()
    tkEqual(libSmartFilter(view), "sort=titleSort")
    tkEqual(libSmartFilterWords(view), ["By title"])
    view.genre = "Science Fiction"
    view.year = 1999
    view.watched = "false"
    view.sort = "year"
    view.order = "desc"
    tkEqual(libSmartFilter(view), "genre=Science+Fiction&year=1999&unwatched=1&sort=year%3Adesc", "as the other apps write it")
    tkEqual(libSmartFilterWords(view), ["Genre: Science Fiction", "From 1999", "Unwatched", "Newest first"])
    view.genre = "Action & Adventure"
    view.year = 0
    view.watched = "true"
    view.sort = "release"
    view.order = "asc"
    tkEqual(libSmartFilter(view), "genre=Action+%26+Adventure&unwatched=0&sort=originallyAvailableAt")
    tkEqual(libSmartFilterWords(view), ["Genre: Action & Adventure", "Watched", "Earliest release first"])
    view.genre = "Drama"
    view.year = 1999
    view.resolution = "4k"
    view.dynamic_range = "hdr"
    view.watched = "false"
    view.sort = "bitrate"
    view.order = "desc"
    tkEqual(libSmartFilter(view), "genre=Drama&year=1999&resolution=4k&dynamicRange=hdr&unwatched=1&sort=mediaBitrate%3Adesc", "after year, before unwatched")
    tkEqual(libSmartFilterWords(view), ["Genre: Drama", "From 1999", "4K", "Any HDR", "Unwatched", "Highest bitrate first"])
    view.resolution = ""
    view.dynamic_range = "dolby_vision"
    view.genre = ""
    view.year = 0
    view.watched = ""
    tkEqual(libSmartFilter(view), "dynamicRange=dolby_vision&sort=mediaBitrate%3Adesc")
    tkEqual(libSmartFilterWords(view), ["Dolby Vision", "Highest bitrate first"])
    tkEqual(libPlexSort("added"), "addedAt")
    tkEqual(libPlexSort("user_rating"), "userRating")
    tkEqual(libPlexSort("last_played"), "lastViewedAt")
    tkEqual(libPlexSort("bitrate"), "mediaBitrate")
end sub

sub test_library_letters_of_titles()
    letters = libLetters()
    tkEqual(letters.Count(), 27)
    tkEqual(letters[0], "#")
    tkEqual(letters[1], "A")
    tkEqual(letters[26], "Z")
    tkEqual(libLetterOf("Alien"), 1)
    tkEqual(libLetterOf("  zodiac"), 26, "lower case, spaces before")
    tkEqual(libLetterOf("Élan"), 5, "accents taken off")
    tkEqual(libLetterOf("çà et là"), 3)
    tkEqual(libLetterOf("Łódź"), 0, "a letter that is not an accented one")
    tkEqual(libLetterOf("2001: A Space Odyssey"), 0, "digits")
    tkEqual(libLetterOf("七人の侍"), 0, "other alphabets")
    tkEqual(libLetterOf(""), 0)
end sub

sub test_library_letter_runs_of_a_page()
    items = [
        { title: "The Abyss", sort_title: "Abyss" },
        { title: "Alien", sort_title: "" },
        { title: "Brazil" },
        "not an item",
        { title: "Cars", sort_title: "Cars" }
    ]
    tkEqual(libLetterRuns(items), [[1, 0], [2, 2], [0, 3], [3, 4]], "the sort title, else the title")
    tkEqual(libLetterRuns(invalid), [])
end sub

sub test_library_alpha_index_across_pages()
    index = libAlphaNew()
    ' "#" 0-1, B 2-4, then the next page: B 5, D 6-8, F 9.
    libAlphaAdd(index, [[0, 0], [2, 2]], 5)
    libAlphaAdd(index, [[2, 0], [4, 1], [6, 4]], 5)
    tkEqual(index.count, 10)
    tkEqual(index.starts, [0, 2, 6, 9], "a run going on over a page's end is one")
    tkTrue(libAlphaHas(index, 0))
    tkTrue(libAlphaHas(index, 2))
    tkFalse(libAlphaHas(index, 1), "no A")
    tkEqual(libAlphaJump(index, 4), 6, "its first title")
    tkEqual(libAlphaJump(index, 3), 6, "none: the next letter's")
    tkEqual(libAlphaJump(index, 1), 2)
    tkEqual(libAlphaJump(index, 26), 9, "none after: the last letter's before it")
    tkEqual(libAlphaLetterAt(index, 0), 0)
    tkEqual(libAlphaLetterAt(index, 5), 2)
    tkEqual(libAlphaLetterAt(index, 7), 4)
    tkEqual(libAlphaLetterAt(index, 40), 6, "past the end: the last run")
    tkEqual(libAlphaJump(libAlphaNew(), 5), 0, "an empty listing")
    tkEqual(libAlphaLetterAt(libAlphaNew(), 3), 0)
end sub

sub test_library_shows_the_jump_sorted_by_title()
    view = libDefaultView()
    tkTrue(libShowsJump(view, 31))
    tkFalse(libShowsJump(view, 30), "30 titles or fewer")
    view.order = "desc"
    tkTrue(libShowsJump(view, 200), "Z to A too")
    view.sort = "year"
    tkFalse(libShowsJump(view, 200))
end sub
