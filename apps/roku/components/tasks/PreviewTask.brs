' Fetches the seek previews' BIF once, then cuts out the frame for each
' position the player asks for, into previews/{runId}-{n}.jpg beside it
' (Previews.brs). They go in cachefs:, which doesn't count against the
' app's memory as tmp: does (a long film's BIF is tens of MB); the Roku may
' clear it, which only loses a preview. Without it, tmp:. A frame cut once is kept for as long as the task runs;
' its files go when it stops. The player has one task at a time: a task
' starting clears what an earlier one left (one dropped before it could).

sub init()
    m.top.functionName = "previewRun"
end sub

sub previewRun()
    port = CreateObject("roMessagePort")
    m.top.observeField("want", port)
    m.top.observeField("quit", port)
    ' Told to stop before it listened.
    if m.top.quit then return
    fs = CreateObject("roFileSystem")
    folder = "cachefs:/previews"
    if not fs.Exists(folder) and not fs.CreateDirectory(folder) then folder = "tmp:/previews"
    if fs.Exists(folder)
        for each name in fs.GetDirectoryListing(folder)
            fs.Delete(folder + "/" + name)
        end for
    else
        fs.CreateDirectory(folder)
    end if
    ' Another task's files would be taken for this one's.
    runId = CreateObject("roDeviceInfo").GetRandomUUID()
    path = folder + "/" + runId + ".bif"
    if not previewFetch(m.top.url, m.top.userAgent, path, port)
        previewClean(fs, folder, runId)
        return
    end if
    head = CreateObject("roByteArray")
    head.ReadFile(path, 0, 64)
    header = prvHeader(head)
    offsets = invalid
    if header <> invalid
        index = CreateObject("roByteArray")
        index.ReadFile(path, 64, prvIndexLength(header))
        offsets = prvOffsets(index, header.count)
    end if
    if offsets = invalid
        previewClean(fs, folder, runId)
        return
    end if
    m.top.ready = true
    cut = {}
    ' A position asked for before the file came.
    pending = m.top.want
    if pending > 0 then previewCut(header, offsets, path, folder, runId, pending, cut)
    while true
        msg = wait(0, port)
        if type(msg) = "roSGNodeEvent"
            if msg.getField() = "quit" then exit while
            if msg.getField() = "want"
                ' Only the newest of the positions asked for meanwhile: a
                ' slow Roku keeps up with a held key.
                ms = msg.getData()
                newer = port.PeekMessage()
                while newer <> invalid and type(newer) = "roSGNodeEvent" and newer.getField() = "want"
                    newer = port.GetMessage()
                    ms = newer.getData()
                    newer = port.PeekMessage()
                end while
                previewCut(header, offsets, path, folder, runId, ms, cut)
            end if
        end if
    end while
    previewClean(fs, folder, runId)
end sub

' The BIF into path; false when it isn't there (404 while the server makes
' the previews, a network error) or the player quit meanwhile.
function previewFetch(url as string, userAgent as string, path as string, port as object) as boolean
    if url = "" then return false
    transfer = CreateObject("roUrlTransfer")
    transfer.setMessagePort(port)
    transfer.setUrl(url)
    if userAgent <> "" then transfer.AddHeader("User-Agent", userAgent)
    if LCase(Left(url, 8)) = "https://"
        transfer.SetCertificatesFile("common:/certs/ca-bundle.crt")
        transfer.InitClientCertificates()
    end if
    if not transfer.AsyncGetToFile(path) then return false
    while true
        msg = wait(0, port)
        if type(msg) = "roUrlEvent"
            return msg.GetResponseCode() = 200
        else if type(msg) = "roSGNodeEvent" and msg.getField() = "quit"
            transfer.AsyncCancel()
            return false
        end if
    end while
    return false
end function

' The frame for ms, cut out once and told.
sub previewCut(header as object, offsets as object, path as string, folder as string, runId as string, ms as integer, cut as object)
    n = prvFrameNumber(header, ms)
    if n < 0 then return
    uri = folder + "/" + runId + "-" + n.ToStr() + ".jpg"
    if not cut.DoesExist(n.ToStr())
        frame = prvFrame(header, offsets, ms)
        if frame = invalid then return
        bytes = CreateObject("roByteArray")
        if not bytes.ReadFile(path, Int(frame.start), Int(frame.length)) then return
        if not bytes.WriteFile(uri) then return
        cut[n.ToStr()] = true
    end if
    m.top.frame = { ms: ms, uri: uri }
end sub

sub previewClean(fs as object, folder as string, runId as string)
    for each name in fs.GetDirectoryListing(folder)
        if Left(name, Len(runId)) = runId then fs.Delete(folder + "/" + name)
    end for
end sub
