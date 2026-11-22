' ContentNodes from plain fields. Used in the API task, so that big lists
' are built off the render thread, and by screens for small ones. Each
' node takes two native calls, whatever its number of fields.

' Sets fields on node, adding the ones it doesn't have.
sub nodesFill(node as object, fields as object)
    nodesSet(node, fields, nodesKnown(node, fields))
end sub

' Which of fields a fresh node has already (ContentNode's own, like title).
' The same for every node of a list, so asked once.
function nodesKnown(node as object, fields as object) as object
    known = {}
    for each key in fields
        if node.hasField(key) then known[key] = true
    end for
    return known
end function

sub nodesSet(node as object, fields as object, known as object)
    own = {}
    added = {}
    owned = 0
    adding = 0
    for each key in fields
        value = fields[key]
        if value <> invalid
            if known.DoesExist(key)
                own[key] = value
                owned = owned + 1
            else
                added[key] = value
                adding = adding + 1
            end if
        end if
    end for
    if owned > 0 then node.setFields(own)
    if adding > 0 then node.addFields(added)
end sub

' A ContentNode with a child for each of items.
function nodesContent(items as object) as object
    root = CreateObject("roSGNode", "ContentNode")
    nodesAppend(root, items)
    return root
end function

' The nodes are filled before they join root, together: a list showing
' root binds its tiles to children as they come, and a tile reads their
' fields then (Request more seasons, Look further, skeletons).
sub nodesAppend(root as object, items as object)
    known = invalid
    nodes = []
    for each item in items
        node = CreateObject("roSGNode", "ContentNode")
        if known = invalid then known = nodesKnown(node, item)
        nodesSet(node, item, known)
        nodes.Push(node)
    end for
    if nodes.Count() > 0 then root.appendChildren(nodes)
end sub

' Home's rows: a child per row (its title), with a child per tile.
function nodesRows(rows as object) as object
    root = CreateObject("roSGNode", "ContentNode")
    for each row in rows
        node = root.createChild("ContentNode")
        node.title = row.title
        nodesAppend(node, row.tiles)
    end for
    return root
end function

' The Live TV guide's TimeGrid content: a child per channel (its fields),
' with a child per program.
function nodesGuide(rows as object) as object
    root = CreateObject("roSGNode", "ContentNode")
    for each row in rows
        node = root.createChild("ContentNode")
        nodesFill(node, row.fields)
        nodesAppend(node, row.programs)
    end for
    return root
end function
