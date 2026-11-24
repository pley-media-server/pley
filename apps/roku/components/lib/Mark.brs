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

' The glyph for a tile of kind, detail saying more of it: a TMDB title's
' (kind "discover") is its tmdbKind's, "movie" or "show", a TMDB season's a
' season's, and a playlist's music's when it holds music (its kind "audio").
function markTileGlyph(kind as string, detail as string) as string
    if kind = "discover" then return markGlyph(detail)
    if kind = "discoverSeason" then return markGlyph("season")
    if kind = "playlist" and detail = "audio" then return markGlyph("track")
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
