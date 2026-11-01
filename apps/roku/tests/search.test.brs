' Search's rows by kind.

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
    tkEqual(keys, ["person", "channel", "movie", "show", "episode", "album"], "the other apps' order; extras left out")
    tkEqual(rows[0].title, "People")
    tkEqual(rows[0].tiles.Count(), 1)
    tkEqual(rows[0].tiles[0].kind, "person")
    tkEqual(rows[1].title, "Live TV")
    tkEqual(rows[1].tiles[0].channelId, 1)
    tkEqual(rows[1].tiles[0].line, "Morning Briefing", "what's on")
    tkEqual(rows[2].tiles[0].title, "Arrival")
    tkFalse(rows[2].wide)
    tkTrue(rows[4].wide, "episodes are wide")
    tkTrue(rows[4].tiles[0].wide)
    tkEqual(rows[4].tiles[0].subtitle, "S1 E1 · Good News About Hell")
    tkTrue(rows[5].square, "music is square")
    tkEqual(searchRows(invalid, contentTestServer(), 0, true), [])
end sub

sub test_search_row_sizes()
    tkEqual(searchRowSize("movie"), { size: [196, 370], height: 430 })
    tkEqual(searchRowSize("person"), { size: [196, 370], height: 430 })
    tkEqual(searchRowSize("channel"), { size: [220, 240], height: 300 })
    tkEqual(searchRowSize("episode"), { size: [300, 250], height: 310 })
    tkEqual(searchRowSize("track"), { size: [196, 280], height: 340 })
end sub
