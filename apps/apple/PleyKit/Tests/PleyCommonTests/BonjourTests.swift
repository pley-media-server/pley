import Foundation
import Network
import PleyCommon
import Testing

@Suite struct BonjourTXTTests {
    let valid = ["pley": "1", "id": "3f2a9c1e-77b0-4d5e-9a1b-2c3d4e5f6a7b", "name": "Living Room", "version": "0.42.0"]

    @Test func aValidRecord() throws {
        let found = try #require(BonjourTXT.parse(valid, instance: "Living Room (2)"))
        #expect(found == BonjourAnnouncement(instance: "Living Room (2)", id: "3f2a9c1e-77b0-4d5e-9a1b-2c3d4e5f6a7b",
                                             name: "Living Room", version: "0.42.0"))
    }

    @Test func anotherProtocolVersionIsRefused() {
        var record = valid
        record["pley"] = "2"
        #expect(BonjourTXT.parse(record, instance: "x") == nil)
        record["pley"] = nil
        #expect(BonjourTXT.parse(record, instance: "x") == nil)
    }

    @Test(arguments: [nil, "", String(repeating: "a", count: 65), "abc def", "abc/def", "ab\tc", "café"])
    func badIDsAreRefused(_ id: String?) {
        var record = valid
        record["id"] = id
        #expect(BonjourTXT.parse(record, instance: "x") == nil)
    }

    @Test func aSixtyFourCharacterIDIsKept() {
        var record = valid
        record["id"] = String(repeating: "A", count: 63) + "-"
        #expect(BonjourTXT.parse(record, instance: "x")?.id.count == 64)
    }

    @Test func controlAndFormatCharactersLeaveTheName() {
        var record = valid
        record["name"] = " Living\u{0} Room\n\u{202E}evil \u{7}"
        #expect(BonjourTXT.parse(record, instance: "x")?.name == "Living Roomevil")
    }

    @Test func aLongNameIsCut() {
        var record = valid
        record["name"] = String(repeating: "n", count: 300)
        #expect(BonjourTXT.parse(record, instance: "x")?.name == String(repeating: "n", count: 64))
    }

    @Test func theInstanceNameStandsInForAMissingName() {
        var record = valid
        record["name"] = nil
        #expect(BonjourTXT.parse(record, instance: "Attic")?.name == "Attic")
        record["name"] = "\u{0}\u{1} "
        #expect(BonjourTXT.parse(record, instance: "Attic")?.name == "Attic")
    }

    @Test(arguments: ["", String(repeating: "1", count: 33), "0.42 beta", "0.42\t0", "0.42.é"])
    func aBadVersionIsDropped(_ version: String) {
        var record = valid
        record["version"] = version
        let found = BonjourTXT.parse(record, instance: "x")
        #expect(found != nil)
        #expect(found?.version == nil)
    }

    @Test func otherKeysAreIgnored() {
        var record = valid
        record["port"] = "7539"
        record["path"] = "/evil"
        #expect(BonjourTXT.parse(record, instance: "x")?.name == "Living Room")
    }

    @Test func keysAreCaseInsensitive() {
        let record = ["PLEY": "1", "Id": "abc", "Name": "Attic"]
        #expect(BonjourTXT.parse(record, instance: "x")?.name == "Attic")
    }

    @Test func anOversizedRecordIsRefused() {
        var record = valid
        for n in 0..<13 { record["k\(n)"] = "v" }
        #expect(record.count == 17)
        #expect(BonjourTXT.parse(record, instance: "x") == nil)
        record = valid
        record["big"] = String(repeating: "b", count: 1250)
        #expect(BonjourTXT.parse(record, instance: "x") == nil)
        #expect(BonjourTXT.parse(valid, instance: "x", size: 1301) == nil)
        #expect(BonjourTXT.parse(valid, instance: "x", size: 1300) != nil)
    }

    @Test func aRealRecordReadsBack() throws {
        let txt = NWTXTRecord(valid)
        let found = BonjourTXT.parse(txt.dictionary, instance: "Living Room", size: txt.data.count)
        #expect(found?.id == valid["id"])
    }
}

@Suite struct ResolvedAddressTests {
    @Test func privateAddressesAreKept() {
        let v4 = BonjourBrowser.address(of: .hostPort(host: "192.168.1.20", port: 7539))
        #expect(v4 == ResolvedService(host: "192.168.1.20", port: 7539))
        #expect(v4?.httpOrigin == "http://192.168.1.20:7539")
        let v6 = BonjourBrowser.address(of: .hostPort(host: "fd12::5", port: 7539))
        #expect(v6?.httpOrigin == "http://[fd12::5]:7539")
        let mapped = BonjourBrowser.address(of: .hostPort(host: "::ffff:10.0.0.4", port: 80))
        #expect(mapped == ResolvedService(host: "10.0.0.4", port: 80))
    }

    @Test func publicAndLinkLocalAddressesAreNot() {
        #expect(BonjourBrowser.address(of: .hostPort(host: "8.8.8.8", port: 7539)) == nil)
        #expect(BonjourBrowser.address(of: .hostPort(host: "2001:db8::1", port: 7539)) == nil)
        #expect(BonjourBrowser.address(of: .hostPort(host: "fe80::1%lo0", port: 7539)) == nil)
        #expect(BonjourBrowser.address(of: .hostPort(host: "nas.local", port: 7539)) == nil)
    }
}
