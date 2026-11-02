' Addresses: what people type, servers' origins, query strings and the
' server's picture addresses. Plain BrightScript with no roUrlTransfer, so
' the render thread uses it as much as the tasks do.

' pley's web port, for addresses typed without one.
function urlDefaultPort() as integer
    return 7539
end function

' The parts of an http(s) address: { scheme, host, port, rest }, or invalid.
' scheme and host are lower case, an IPv6 host keeps its brackets, port is
' -1 when there is none, rest is the path and query as written. Addresses
' with a user name are refused.
function urlParse(text as string) as dynamic
    re = CreateObject("roRegex", "^([a-zA-Z][a-zA-Z0-9+.\-]*)://(\[[0-9a-fA-F:.]+\]|[^/?#:@\[\]\s]+)(:([0-9]*))?([/?#].*)?$", "")
    groups = re.Match(text)
    if groups.Count() < 3 then return invalid
    port = -1
    digits = urlGroup(groups, 4)
    if digits <> ""
        if Len(digits) > 5 then return invalid
        port = digits.ToInt()
        if port < 1 or port > 65535 then return invalid
    end if
    return {
        scheme: LCase(groups[1]),
        host: LCase(groups[2]),
        port: port,
        rest: urlGroup(groups, 5)
    }
end function

' A group of a roRegex match, "" when it didn't take part.
function urlGroup(groups as object, index as integer) as string
    if index >= groups.Count() then return ""
    value = groups[index]
    if value = invalid then return ""
    return value
end function

' scheme://host[:port], leaving out the scheme's own port.
function urlOriginOf(scheme as string, host as string, port as integer) as string
    if port = -1 or (scheme = "http" and port = 80) or (scheme = "https" and port = 443)
        return scheme + "://" + host
    end if
    return scheme + "://" + host + ":" + port.ToStr()
end function

' The origin of an http(s) address, or invalid.
function urlOrigin(text as string) as dynamic
    parts = urlParse(text)
    if parts = invalid then return invalid
    if parts.scheme <> "http" and parts.scheme <> "https" then return invalid
    return urlOriginOf(parts.scheme, parts.host, parts.port)
end function

function urlHasScheme(text as string) as boolean
    re = CreateObject("roRegex", "^[a-zA-Z][a-zA-Z0-9+.\-]*://", "")
    return re.IsMatch(text)
end function

' A server's address (its origin) for what someone typed: "nas",
' "nas:8080", "192.168.1.20" or "https://pley.example.com/". Without a
' scheme it is http, and without a port pley's. invalid when it can't be
' one. The same rules as the Android and desktop apps' pickers.
function urlNormalizeServer(input as string) as dynamic
    text = input.Trim()
    if text = "" then return invalid
    space = CreateObject("roRegex", "\s", "")
    if space.IsMatch(text) then return invalid
    typedScheme = urlHasScheme(text)
    if not typedScheme then text = "http://" + text
    parts = urlParse(text)
    if parts = invalid then return invalid
    if parts.scheme <> "http" and parts.scheme <> "https" then return invalid
    port = parts.port
    if port = -1 and not typedScheme then port = urlDefaultPort()
    return urlOriginOf(parts.scheme, parts.host, port)
end function

' Whether host is this device, on its local network (a private address, a
' name without dots, .local and the like) or a Tailscale name: where plain
' http is how pley is reached.
function urlIsLocalHost(host as string) as boolean
    h = LCase(host)
    if Left(h, 1) = "[" and Right(h, 1) = "]" then h = Mid(h, 2, Len(h) - 2)
    if h = "localhost" then return true
    if Instr(1, h, ".") = 0 and Instr(1, h, ":") = 0 then return true
    suffix = CreateObject("roRegex", "\.(local|lan|home|internal|localdomain|home\.arpa|ts\.net)$", "")
    if suffix.IsMatch(h) then return true
    v4 = CreateObject("roRegex", "^([0-9]+)\.([0-9]+)\.[0-9]+\.[0-9]+$", "")
    octets = v4.Match(h)
    if octets.Count() = 3
        a = octets[1].ToInt()
        b = octets[2].ToInt()
        return a = 10 or a = 127 or (a = 172 and b >= 16 and b <= 31) or (a = 192 and b = 168) or (a = 169 and b = 254) or (a = 100 and b >= 64 and b <= 127)
    end if
    if h = "::1" then return true
    uniqueLocal = CreateObject("roRegex", "^f[cd][0-9a-f]{0,2}:", "")
    linkLocal = CreateObject("roRegex", "^fe[89ab][0-9a-f]?:", "")
    return uniqueLocal.IsMatch(h) or linkLocal.IsMatch(h)
end function

' Whether a connection to url travels unencrypted beyond the local network.
function urlUnencrypted(url as string) as boolean
    parts = urlParse(url)
    if parts = invalid then return false
    return parts.scheme = "http" and not urlIsLocalHost(parts.host)
end function

' The addresses to try, in turn, for what someone typed: a name on the
' internet typed without http:// is tried with https first (pley behind a
' proxy with a certificate), then with plain http on pley's port.
function urlCandidates(input as string) as object
    url = urlNormalizeServer(input)
    if url = invalid then return []
    text = input.Trim()
    beforePath = text
    slash = Instr(1, text, "/")
    if slash > 0 then beforePath = Left(text, slash - 1)
    portTyped = CreateObject("roRegex", ":[0-9]+$", "")
    host = urlParse(url).host
    if urlHasScheme(text) or portTyped.IsMatch(beforePath) or urlIsLocalHost(host) then return [url]
    return ["https://" + host, url]
end function

' The address to show for a server: its host and, unless it's pley's own
' port over http, its port.
function urlDisplayAddress(url as string) as string
    parts = urlParse(url)
    if parts = invalid then return url
    if parts.port = -1 or (parts.scheme = "http" and parts.port = urlDefaultPort()) then return parts.host
    return parts.host + ":" + parts.port.ToStr()
end function

' Percent-encoding of everything but RFC 3986's unreserved characters, of
' the UTF-8 bytes.
function urlEncode(text as string) as string
    if httpCache().unreserved.IsMatch(text) then return text
    bytes = CreateObject("roByteArray")
    bytes.FromAsciiString(text)
    out = ""
    for i = 0 to bytes.Count() - 1
        b = bytes[i]
        if (b >= 48 and b <= 57) or (b >= 65 and b <= 90) or (b >= 97 and b <= 122) or b = 45 or b = 46 or b = 95 or b = 126
            out = out + Chr(b)
        else
            out = out + "%" + urlHexByte(b)
        end if
    end for
    return out
end function

function urlHexByte(b as integer) as string
    digits = "0123456789ABCDEF"
    return Mid(digits, Int(b / 16) + 1, 1) + Mid(digits, (b mod 16) + 1, 1)
end function

' A query string's key or value as it reads: "+" a space, percent-encoded
' UTF-8 bytes their characters; a "%" without two hex digits stays.
function urlDecode(text as string) as string
    if Instr(1, text, "%") = 0 then return text.Replace("+", " ")
    escaped = CreateObject("roRegex", "^%[0-9A-Fa-f]{2}", "")
    plain = CreateObject("roByteArray")
    hex = ""
    i = 1
    n = Len(text)
    while i <= n
        c = Mid(text, i, 1)
        if c = "%" and escaped.IsMatch(Mid(text, i, 3))
            hex = hex + Mid(text, i + 1, 2)
            i = i + 3
        else
            if c = "+" then c = " "
            plain.FromAsciiString(c)
            hex = hex + plain.ToHexString()
            i = i + 1
        end if
    end while
    bytes = CreateObject("roByteArray")
    bytes.FromHexString(hex)
    return bytes.ToAsciiString()
end function

' A value as it goes in a query string or form.
function urlValue(value as dynamic) as string
    if GetInterface(value, "ifString") <> invalid then return value
    if GetInterface(value, "ifBoolean") <> invalid
        if value then return "true"
        return "false"
    end if
    if GetInterface(value, "ifInt") <> invalid or GetInterface(value, "ifLongInt") <> invalid then return value.ToStr()
    if GetInterface(value, "ifFloat") <> invalid or GetInterface(value, "ifDouble") <> invalid
        if value = Int(value) then return Int(value).ToStr()
        return Str(value).Trim()
    end if
    return ""
end function

' "a=1&q=a%20b": the keys in order, invalid values left out. Also the body
' of an application/x-www-form-urlencoded request.
function urlQuery(params as dynamic) as string
    if params = invalid then return ""
    pairs = []
    for each key in params.Keys()
        value = params[key]
        if value <> invalid then pairs.Push(urlEncode(key) + "=" + urlEncode(urlValue(value)))
    end for
    return pairs.Join("&")
end function

' path with the query of params, if any.
function urlWithQuery(path as string, params as dynamic) as string
    query = urlQuery(params)
    if query = "" then return path
    if Instr(1, path, "?") > 0 then return path + "&" + query
    return path + "?" + query
end function

' The address of one of the server's pictures (a poster_url such as
' "/api/items/12/images/poster?v=1729600000"), asking for width pixels.
' "" without a picture.
function urlImage(server as string, path as dynamic, width as integer) as string
    if GetInterface(path, "ifString") = invalid or path = "" then return ""
    url = path
    if Left(path, 1) = "/" then url = server + path
    if Instr(1, url, "?") > 0 then return url + "&w=" + width.ToStr()
    return url + "?w=" + width.ToStr()
end function
