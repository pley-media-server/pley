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
    section.Write(key, "not json")
    tkEqual(libLoadView(section, key, "movies"), libDefaultView())
    section.Delete(key)
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
