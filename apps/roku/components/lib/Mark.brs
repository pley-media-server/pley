' Pley's mark on a tile without a picture: the logo's three posters, a
' glyph cut out of the front one for the title's kind of library. Every
' app draws it from packages/placeholder/placeholder.json; this app ships it
' as pictures (scripts/render-placeholder-marks.mjs at the repository's
' root), and tests/mark.test.brs holds this file to placeholder.json. Kept apart
' from Content.brs so that each tile loads only this.

' The glyph for an item of kind, the API's: its library's, or play.
function markGlyph(kind as string) as string
    if kind = "movie" then return "movies"
    if kind = "show" or kind = "season" or kind = "episode" then return "shows"
    if kind = "artist" or kind = "album" or kind = "track" then return "music"
    if kind = "audiobook" or kind = "part" then return "audiobooks"
    return "play"
end function

' The glyph for a tile of kind: a TMDB title's (kind "discover") is its
' tmdbKind's, "movie" or "show", and a TMDB season's a season's.
function markTileGlyph(kind as string, tmdbKind as string) as string
    if kind = "discover" then return markGlyph(tmdbKind)
    if kind = "discoverSeason" then return markGlyph("season")
    return markGlyph(kind)
end function

' The mark with glyph in a picture of a tile's shape (a poster, a 16:9
' picture or a square), so that it scales without stretching.
function markPlaceholderUri(glyph as string, wide as boolean, square as boolean) as string
    shape = "poster"
    if wide
        shape = "wide"
    else if square
        shape = "square"
    end if
    return "pkg:/images/placeholder-" + shape + "-" + glyph + ".png"
end function
