import Foundation
import PleyCommon
import Testing

/// The Android app's ServersTest, case for case.
@Suite struct ServersTests {
    @Test func addressesGetHttpAndPleysPort() {
        #expect(Servers.normalize("nas") == "http://nas:7539")
        #expect(Servers.normalize(" 192.168.1.20 ") == "http://192.168.1.20:7539")
        #expect(Servers.normalize("nas:8080") == "http://nas:8080")
        #expect(Servers.normalize("[fd00::5]") == "http://[fd00::5]:7539")
    }

    @Test func spelledOutAddressesKeepTheirSchemeAsOrigins() {
        #expect(Servers.normalize("https://pley.example.com/") == "https://pley.example.com")
        #expect(Servers.normalize("http://nas") == "http://nas")
        #expect(Servers.normalize("HTTP://NAS:7539/library/x") == "http://nas:7539")
        #expect(Servers.normalize("https://pley.example.com:443") == "https://pley.example.com")
    }

    @Test(arguments: ["", "   ", "ftp://nas", "file:///sdcard/x", "http://", "two words", "javascript:alert(1)",
                      "http://[fe80::1%25en0]", "http://nas:70000"])
    func whatCantBeAServerIsRefused(_ bad: String) {
        #expect(Servers.normalize(bad) == nil)
    }

    @Test func addressesLeavePleysPortOut() {
        #expect(Servers.displayAddress("http://192.168.1.20:7539") == "192.168.1.20")
        #expect(Servers.displayAddress("http://nas:8080") == "nas:8080")
        #expect(Servers.displayAddress("https://pley.example.com") == "pley.example.com")
        #expect(Servers.displayAddress("http://[fd00::5]:7539") == "[fd00::5]")
    }

    @Test(arguments: ["localhost", "nas", "nas.local", "pley.lan", "192.168.1.20", "10.0.0.7", "172.20.0.1",
                      "100.101.102.103", "box.tail1234.ts.net", "[fd00::5]", "fe80::1", "::1"])
    func localHosts(_ host: String) {
        #expect(Servers.isLocalHost(host))
    }

    @Test(arguments: ["pley.example.com", "203.0.113.9", "172.32.0.1", "100.128.0.1", "[2001:db8::1]"])
    func hostsBeyondTheLocalNetwork(_ host: String) {
        #expect(!Servers.isLocalHost(host))
    }

    @Test func unencryptedAddressesAreFineOnTheLocalNetworkOnly() {
        #expect(Servers.unencrypted("http://pley.example.com:7539"))
        #expect(!Servers.unencrypted("https://pley.example.com"))
        #expect(!Servers.unencrypted("http://192.168.1.20:7539"))
    }

    @Test func namesOnTheInternetAreTriedWithHttpsFirst() {
        #expect(Servers.candidates("pley.example.com") == ["https://pley.example.com", "http://pley.example.com:7539"])
        #expect(Servers.candidates("nas") == ["http://nas:7539"])
        #expect(Servers.candidates("192.168.1.20") == ["http://192.168.1.20:7539"])
        #expect(Servers.candidates("http://pley.example.com") == ["http://pley.example.com"])
        #expect(Servers.candidates("pley.example.com:8443") == ["http://pley.example.com:8443"])
        #expect(Servers.candidates("two words") == [])
    }

    private let saved = [
        SavedServer(origin: "http://nas:7539", name: "NAS", id: "a1", lastUsed: Date(timeIntervalSince1970: 10)),
        SavedServer(origin: "http://office:7539", name: "Office", id: nil, lastUsed: Date(timeIntervalSince1970: 20)),
    ]

    @Test func aNewServerNamedLikeASavedOneIsFlagged() throws {
        let found = [FoundServer(origin: "http://192.168.1.66:7539", name: "nas ", id: "evil", version: "0.1.0")]
        let entry = try #require(Servers.merge(saved: saved, found: found).last)
        #expect(!entry.saved)
        #expect(entry.lookalike)
    }

    @Test func savedServersComeFirstTheLastOpenedOnTop() {
        let found = [
            FoundServer(origin: "http://192.168.1.20:7539", name: "NAS", id: "a1", version: "0.1.0"),
            FoundServer(origin: "http://192.168.1.30:7539", name: "Den", id: "b2", version: "0.1.0"),
        ]
        #expect(Servers.merge(saved: saved, found: found) == [
            ServerEntry(origin: "http://office:7539", name: "Office", id: nil, saved: true, found: false),
            ServerEntry(origin: "http://nas:7539", name: "NAS", id: "a1", saved: true, found: true),
            ServerEntry(origin: "http://192.168.1.30:7539", name: "Den", id: "b2", saved: false, found: true),
        ])
    }

    @Test func serversWithoutAnIdMatchByAddress() {
        let found = [FoundServer(origin: "http://office:7539", name: "Office", id: "c3", version: "0.1.0")]
        let flags = Servers.merge(saved: saved, found: found).map { [$0.saved, $0.found] }
        #expect(flags == [[true, true], [true, false]])
        #expect(Servers.lastUsed(saved)?.name == "Office")
        #expect(Servers.lastUsed([]) == nil)
    }

    @Test func savingReplacesTheSameAddressOrId() {
        var list = Servers.withServer(
            saved, SavedServer(origin: "http://office:7539", name: "Office", id: nil, lastUsed: Date(timeIntervalSince1970: 30)))
        #expect(list.map(\.origin) == ["http://nas:7539", "http://office:7539"])
        #expect(list.map(\.lastUsed.timeIntervalSince1970) == [10, 30])
        // The same server at a new address.
        list = Servers.withServer(
            list, SavedServer(origin: "http://192.168.1.5:7539", name: "NAS", id: "a1", lastUsed: Date(timeIntervalSince1970: 40)))
        #expect(list.map(\.origin) == ["http://office:7539", "http://192.168.1.5:7539"])
    }
}

@Suite struct AddressesTests {
    @Test func originsAreWrittenAsBrowsersDo() {
        #expect(Addresses.origin("http://NAS:7539/library?x=1") == "http://nas:7539")
        #expect(Addresses.origin("https://pley.example.com:443/") == "https://pley.example.com")
        #expect(Addresses.origin("http://192.168.1.20:80") == "http://192.168.1.20")
        #expect(Addresses.origin("http://[::1]:7539") == "http://[::1]:7539")
        #expect(Addresses.origin("http://[FD00::5]/x") == "http://[fd00::5]")
        #expect(Addresses.origin(URL(string: "HTTPS://Pley.Example.com:8443/api")!) == "https://pley.example.com:8443")
    }

    @Test(arguments: ["nas:7539", "not a url", "http://[fe80::1%25en0]:7539", "http://", "/api/server"])
    func whatHasNoOriginIsRefused(_ text: String) {
        #expect(Addresses.origin(text) == nil)
    }
}
