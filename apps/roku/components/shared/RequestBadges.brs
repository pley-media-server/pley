' The badges of TMDB's titles where they show (Discover, its lists,
' search), as their requests change. On the render thread, with
' lib/Requests.brs.

' A title's request changed (note: { key, status, libraryId }): its tiles
' in content (a list's, or rows of lists) get the badge it has now. Each
' is replaced by a copy, which the list draws again.
sub uiPatchRequestTiles(content as dynamic, note as object)
    if content = invalid then return
    for i = 0 to content.getChildCount() - 1
        node = content.getChild(i)
        if node.getChildCount() > 0
            uiPatchRequestTiles(node, note)
        else if node.hasField("key") and node.hasField("tmdbId") and node.key = note.key
            fields = {}
            for each name in ["title", "subtitle", "image", "wide", "progress", "watched", "itemId", "kind", "tmdbKind", "tmdbId", "key"]
                if node.hasField(name) then fields[name] = node.getField(name)
            end for
            status = httpString(note.status)
            libraryId = httpNumber(note.libraryId, 0)
            fields.status = status
            fields.libraryId = libraryId
            fields.badge = reqTitleBadge(status, libraryId)
            fields.badgeColor = reqBadgeColor(status, libraryId)
            fresh = CreateObject("roSGNode", "ContentNode")
            nodesFill(fresh, fields)
            content.replaceChild(fresh, i)
        end if
    end for
end sub
