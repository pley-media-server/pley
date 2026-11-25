' Pley's mark on a tile without a picture (Mark.brs), held to the one
' definition every app draws it from (packages/placeholder/
' placeholder.json, which the runner gives as pkg:/placeholder/placeholder.json),
' and its pictures, which scripts/render-placeholder-marks.mjs draws.

function markTestMarks() as object
    marks = ParseJson(ReadAsciiFile("pkg:/placeholder/placeholder.json"))
    if marks = invalid or GetInterface(marks.kinds, "ifAssociativeArray") = invalid or GetInterface(marks.glyphs, "ifAssociativeArray") = invalid
        tkFail("packages/placeholder/placeholder.json didn't load")
        return invalid
    end if
    return marks
end function

sub test_mark_glyph_by_kind_is_the_placeholder_files()
    marks = markTestMarks()
    if marks = invalid then return
    for each kind in marks.kinds
        tkEqual(markGlyph(kind), marks.kinds[kind], kind)
    end for
    ' The kinds it doesn't name take its default.
    for each kind in ["extra", "collection", "playlist", "person", "cast", ""]
        tkEqual(markGlyph(kind), marks.default, "kind " + kind)
    end for
end sub

sub test_mark_a_tmdb_titles_glyph_is_its_kinds()
    tkEqual(markTileGlyph("discover", "movie"), "movies")
    tkEqual(markTileGlyph("discover", "show"), "shows")
    tkEqual(markTileGlyph("discoverSeason", ""), "shows")
    tkEqual(markTileGlyph("album", ""), "music")
    tkEqual(markTileGlyph("person", ""), "play")
    tkEqual(markTileGlyph("playlist", "audio"), "music")
    tkEqual(markTileGlyph("playlist", "video"), "play")
end sub

sub test_mark_every_glyph_has_a_picture_of_each_shape()
    marks = markTestMarks()
    if marks = invalid then return
    files = CreateObject("roFileSystem")
    for each glyph in marks.glyphs
        for each shape in [{ wide: false, square: false }, { wide: true, square: false }, { wide: false, square: true }]
            uri = markPlaceholderUri(glyph, shape.wide, shape.square)
            tkTrue(files.Exists(uri), uri)
        end for
    end for
    tkFalse(files.Exists("pkg:/images/placeholder-poster-none.png"), "a glyph without pictures")
    tkEqual(markPlaceholderUri("movies", false, false), "pkg:/images/placeholder-poster-movies.png")
    tkEqual(markPlaceholderUri("shows", true, false), "pkg:/images/placeholder-wide-shows.png")
    tkEqual(markPlaceholderUri("music", false, true), "pkg:/images/placeholder-square-music.png")
end sub
