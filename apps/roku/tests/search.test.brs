' Search's rows by kind and its recent searches.

sub test_search_rows_by_kind_in_order()
    data = {
        items: [
            contentTestEpisode(),
            contentTestMovie(),
            { id: 5, kind: "album", title: "Blue", parent_title: "The Fixtures" },
            { id: 6, kind: "extra", title: "Trailer" },
            { id: 7, kind: "show", title: "Severance", year: 2022 },
            "not an item"
        ],
        people: [{ id: 9, name: "Ada Lovelace", photo_url: "/api/people/9/photo?v=1" }, { id: 0, name: "No id" }],
        channels: [liveSample()[0]]
    }
    rows = searchRows(data, contentTestServer(), 1500, true)
    keys = []
    for each row in rows
        keys.Push(row.key)
    end for
    tkEqual(keys, ["movie", "show", "episode", "album", "channel", "person"], "the other apps' order; extras left out")
    tkEqual(rows[0].tiles[0].title, "Arrival")
    tkFalse(rows[0].wide)
    tkTrue(rows[2].wide, "episodes are wide")
    tkTrue(rows[2].tiles[0].wide)
    tkEqual(rows[2].tiles[0].subtitle, "S1 E1 · Good News About Hell")
    tkTrue(rows[3].square, "music is square")
    tkEqual(rows[4].title, "Live TV")
    tkEqual(rows[4].tiles[0].channelId, 1)
    tkEqual(rows[4].tiles[0].line, "Morning Briefing", "what's on")
    tkEqual(rows[5].title, "People")
    tkEqual(rows[5].tiles.Count(), 1)
    tkEqual(rows[5].tiles[0].kind, "person")
    tkEqual(searchRows(invalid, contentTestServer(), 0, true), [])
end sub

sub test_search_row_sizes()
    tkEqual(searchRowSize("movie"), { size: [196, 370], height: 430 })
    tkEqual(searchRowSize("person"), { size: [196, 370], height: 430 })
    tkEqual(searchRowSize("channel"), { size: [220, 240], height: 300 })
    tkEqual(searchRowSize("episode"), { size: [300, 250], height: 310 })
    tkEqual(searchRowSize("track"), { size: [196, 280], height: 340 })
end sub

sub test_search_remembers_the_last_ten()
    recent = searchRemember([], "  dune ")
    tkEqual(recent, ["dune"])
    recent = searchRemember(recent, "Arrival")
    tkEqual(recent, ["Arrival", "dune"], "newest first")
    recent = searchRemember(recent, "Dune")
    tkEqual(recent, ["Dune", "Arrival"], "again: to the top, once, as last typed")
    tkEqual(searchRemember(recent, "   "), recent, "nothing typed")
    for i = 1 to 12
        recent = searchRemember(recent, "query " + i.ToStr())
    end for
    tkEqual(recent.Count(), 10)
    tkEqual(recent[0], "query 12")
    tkEqual(recent[9], "query 3")
end sub

sub test_search_recent_kept_per_server_and_profile()
    section = CreateObject("roRegistrySection", "searchesTest")
    key = searchRecentKey("http://nas:7539", 4)
    tkEqual(key, "http://nas:7539|4")
    section.Delete(key)
    tkEqual(searchRecentLoad(section, key), [])
    searchRecentSave(section, key, ["Dune", "Arrival"])
    tkEqual(searchRecentLoad(section, key), ["Dune", "Arrival"])
    section.Write(key, "[""ok"", 3, """", ""fine""]")
    tkEqual(searchRecentLoad(section, key), ["ok", "fine"], "only words")
    section.Write(key, "not json")
    tkEqual(searchRecentLoad(section, key), [])
    searchRecentSave(section, key, [])
    tkFalse(section.Exists(key), "cleared")
end sub
