sub test_url_normalizes_typed_addresses()
    tkEqual(urlNormalizeServer("nas"), "http://nas:7539")
    tkEqual(urlNormalizeServer("nas:8080"), "http://nas:8080")
    tkEqual(urlNormalizeServer(" 192.168.1.20 "), "http://192.168.1.20:7539")
    tkEqual(urlNormalizeServer("https://pley.example.com/"), "https://pley.example.com")
    tkEqual(urlNormalizeServer("https://pley.example.com:443/web"), "https://pley.example.com")
    tkEqual(urlNormalizeServer("HTTP://NAS.local:80/x"), "http://nas.local")
    tkEqual(urlNormalizeServer("http://nas"), "http://nas", "a scheme without a port keeps the scheme's port")
    tkEqual(urlNormalizeServer("[fe80::1]"), "http://[fe80::1]:7539")
    tkEqual(urlNormalizeServer("nas/library"), "http://nas:7539")
end sub

sub test_url_refuses_what_cant_be_a_server()
    tkInvalid(urlNormalizeServer(""))
    tkInvalid(urlNormalizeServer("   "))
    tkInvalid(urlNormalizeServer("two words"))
    tkInvalid(urlNormalizeServer("ftp://nas"))
    tkInvalid(urlNormalizeServer("http://user@nas"))
    tkInvalid(urlNormalizeServer("nas:99999"))
end sub

sub test_url_knows_local_hosts()
    for each host in ["nas", "localhost", "pley.local", "box.tail1234.ts.net", "router.home.arpa", "192.168.1.20", "10.0.0.1", "172.16.0.1", "172.31.255.1", "169.254.1.1", "100.64.1.1", "127.0.0.1", "[::1]", "fd12:3456::1", "fe80::1"]
        tkTrue(urlIsLocalHost(host), host)
    end for
    for each host in ["pley.example.com", "8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:db8::1"]
        tkFalse(urlIsLocalHost(host), host)
    end for
end sub

sub test_url_tries_https_first_for_names_on_the_internet()
    tkEqual(urlCandidates("pley.example.com"), ["https://pley.example.com", "http://pley.example.com:7539"])
    tkEqual(urlCandidates("nas"), ["http://nas:7539"])
    tkEqual(urlCandidates("192.168.1.20"), ["http://192.168.1.20:7539"])
    tkEqual(urlCandidates("pley.example.com:8080"), ["http://pley.example.com:8080"])
    tkEqual(urlCandidates("http://pley.example.com"), ["http://pley.example.com"])
    tkEqual(urlCandidates(""), [])
end sub

sub test_url_shows_addresses_without_pleys_port()
    tkEqual(urlDisplayAddress("http://192.168.1.20:7539"), "192.168.1.20")
    tkEqual(urlDisplayAddress("http://nas:8080"), "nas:8080")
    tkEqual(urlDisplayAddress("https://pley.example.com"), "pley.example.com")
    tkEqual(urlDisplayAddress("https://pley.example.com:7539"), "pley.example.com:7539")
end sub

sub test_url_origins()
    tkEqual(urlOrigin("http://Host:7539/a/b?c=d"), "http://host:7539")
    tkEqual(urlOrigin("https://h:443/"), "https://h")
    tkInvalid(urlOrigin("javascript:alert(1)"))
    tkInvalid(urlOrigin("pley.example.com"))
end sub

sub test_url_flags_plain_http_beyond_the_home_network()
    tkTrue(urlUnencrypted("http://pley.example.com"))
    tkFalse(urlUnencrypted("http://nas:7539"))
    tkFalse(urlUnencrypted("https://pley.example.com"))
end sub

sub test_url_encodes_utf8_and_reserved_characters()
    tkEqual(urlEncode("a b&c=d/é~-_."), "a%20b%26c%3Dd%2F%C3%A9~-_.")
    tkEqual(urlEncode("urn:ietf:params:oauth:grant-type:device_code"), "urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
end sub

sub test_url_decodes_query_values()
    tkEqual(urlDecode("Science+Fiction"), "Science Fiction")
    tkEqual(urlDecode("Com%C3%A9die%20%26%20Drame"), "Comédie & Drame")
    tkEqual(urlDecode("year%3E%3E"), "year>>")
    tkEqual(urlDecode("100%"), "100%", "a % without its digits stays")
    tkEqual(urlDecode("%zz+é"), "%zz é")
    tkEqual(urlDecode(urlEncode("a b&c=d/é~")), "a b&c=d/é~")
end sub

sub test_url_builds_query_strings()
    tkEqual(urlQuery({ q: "star wars", limit: 40, watched: true, missing: invalid }), "limit=40&q=star%20wars&watched=true")
    tkEqual(urlQuery({}), "")
    tkEqual(urlQuery(invalid), "")
    tkEqual(urlWithQuery("/api/search", { q: "dune" }), "/api/search?q=dune")
    tkEqual(urlWithQuery("/api/x?y=1", { a: 1 }), "/api/x?y=1&a=1")
    tkEqual(urlWithQuery("/api/x", {}), "/api/x")
    tkEqual(urlValue(2.5), "2.5")
    tkEqual(urlValue(3.0), "3")
end sub

sub test_url_builds_picture_addresses()
    tkEqual(urlImage("http://nas:7539", "/api/items/12/images/poster?v=1729600000", 300), "http://nas:7539/api/items/12/images/poster?v=1729600000&w=300")
    tkEqual(urlImage("http://nas:7539", "/api/people/7/photo", 200), "http://nas:7539/api/people/7/photo?w=200")
    tkEqual(urlImage("http://nas:7539", invalid, 300), "")
    tkEqual(urlImage("http://nas:7539", "", 300), "")
end sub
