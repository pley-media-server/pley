' A page of text from Settings (its route says which): Diagnostics, the
' app and this Roku, then what the Roku tells the server it plays, as it
' asks for each title (lib/Caps.brs); or the open-source licences: Pley's
' own, then the notices of what the package bundles (licenses/).

sub init()
    m.title = m.top.findNode("title")
    m.text = m.top.findNode("text")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active then return
    if m.text.text = ""
        if m.top.route = "licences"
            m.title.text = "Open-source licences"
            m.text.text = licences()
        else
            m.title.text = "Diagnostics"
            m.text.text = diagnostics()
        end if
    end if
    m.text.setFocus(true)
end sub

function diagnostics() as string
    session = m.global.session
    lines = ["This Roku", session.device, "Pley for Roku " + session.appVersion, "", "What it plays"]
    lines.Append(capsLines(capsBuild(CreateObject("roDeviceInfo"))))
    return lines.Join(Chr(10))
end function

function licences() as string
    own = "Pley for Roku is free software under the GNU Affero General Public License, version 3 (AGPL-3.0-only)."
    return own + Chr(10) + Chr(10) + ReadAsciiFile("pkg:/licenses/bslib.txt")
end function
