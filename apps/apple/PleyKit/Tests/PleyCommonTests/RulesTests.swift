import Foundation
import Network
import PleyCommon
import Testing

@Suite struct PrivateAddressTests {
    @Test(arguments: ["10.0.0.1", "10.255.255.255", "172.16.0.1", "172.31.255.254", "192.168.1.2", "127.0.0.1",
                      "127.8.9.10", "169.254.1.1", "100.64.0.1", "100.127.255.254", "::1", "fe80::1", "fd12::1",
                      "fc00::1", "::ffff:192.168.1.2", "::ffff:127.0.0.1", "[fe80::1%en0]", "[fd00::5]"])
    func privateAddresses(_ literal: String) {
        #expect(PrivateAddress.isPrivate(literal: literal) == true)
    }

    @Test(arguments: ["100.63.255.255", "100.128.0.1", "8.8.8.8", "172.32.0.1", "172.15.0.1", "192.169.0.1",
                      "93.184.216.34", "0.0.0.0", "2001:db8::1", "::ffff:8.8.8.8", "2606:4700::1111", "::"])
    func publicAddresses(_ literal: String) {
        #expect(PrivateAddress.isPrivate(literal: literal) == false)
    }

    @Test(arguments: ["nas", "pley.example.com", "127.1", "localhost", ""])
    func namesAreNotLiterals(_ host: String) {
        #expect(PrivateAddress.isPrivate(literal: host) == nil)
    }

    @Test func parsedAddresses() throws {
        #expect(PrivateAddress.isPrivate(try #require(IPv4Address("192.168.0.9"))))
        #expect(!PrivateAddress.isPrivate(try #require(IPv4Address("1.1.1.1"))))
        #expect(PrivateAddress.isPrivate(try #require(IPv6Address("fe80::1"))))
        #expect(!PrivateAddress.isPrivate(try #require(IPv6Address("2001:db8::1"))))
    }
}

@Suite struct HostResolverTests {
    @Test func theSystemResolvesLoopback() async throws {
        let addresses = try await SystemHostResolver().addresses(of: "localhost")
        #expect(!addresses.isEmpty)
        #expect(addresses.allSatisfy { PrivateAddress.isPrivate(literal: $0) == true })
    }

    @Test func literalsResolveToThemselves() async throws {
        #expect(try await SystemHostResolver().addresses(of: "192.168.4.5") == ["192.168.4.5"])
    }

    @Test func aNameThatDoesntExistFails() async {
        await #expect(throws: HostResolverError.self) {
            try await SystemHostResolver().addresses(of: "nothing.invalid")
        }
    }
}

@Suite struct UserAgentTests {
    @Test func theFourForms() {
        #expect(UserAgent.format(version: "0.1.0", osName: "iPadOS", osVersion: "16.7", device: "iPad")
            == "pleyApple/0.1.0 (iPadOS 16.7; iPad)")
        #expect(UserAgent.format(version: "0.1.0", osName: "iOS", osVersion: "18.1", device: "iPhone")
            == "pleyApple/0.1.0 (iOS 18.1; iPhone)")
        #expect(UserAgent.format(version: "0.1.0", osName: "tvOS", osVersion: "18.2", device: "Apple TV")
            == "pleyApple/0.1.0 (tvOS 18.2; Apple TV)")
        #expect(UserAgent.format(version: "0.1.0", osName: "macOS", osVersion: "15.2", device: "Mac")
            == "pleyApple/0.1.0 (macOS 15.2; Mac)")
    }

    @Test func aDeviceAHeaderCantCarryIsLeftOut() {
        #expect(UserAgent.format(version: "0.1.0", osName: "iOS", osVersion: "18.1", device: "📺")
            == "pleyApple/0.1.0 (iOS 18.1)")
    }

    @Test func namesAreWhatAHeaderCarries() {
        #expect(UserAgent.cleaned("Anna’s TV — (Living room);") == "Anna's TV - Living room")
        #expect(UserAgent.cleaned("  Den\tTV 📺 ") == "Den TV")
        #expect(UserAgent.cleaned("📺") == "")
        #expect(UserAgent.cleaned("Télé du salon") == "Tele du salon")
        #expect(UserAgent.cleaned(#"a\b;c"#) == "a b c")
        #expect(UserAgent.cleaned(String(repeating: "x", count: 100)).count == 40)
    }

    @Test @MainActor func thisDevicesAgent() {
        let agent = UserAgent.current
        #expect(agent.hasPrefix("pleyApple/"))
        #if os(macOS)
        #expect(agent.hasSuffix("; Mac)"))
        #elseif os(tvOS)
        #expect(agent.contains("(tvOS "))
        #endif
    }
}

@Suite struct PlainTests {
    @Test func serverTextBecomesOneSafeLine() {
        #expect(plain("  item 42\nnot found\u{0007} ") == "item 42not found")
        #expect(plain("abc\u{202E}def\u{200B}") == "abcdef")
        #expect(plain(String(repeating: "y", count: 500)).count == 200)
        #expect(plain("Anna’s café") == "Anna’s café")
    }
}
