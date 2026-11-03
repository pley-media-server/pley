import Foundation
import PleyModel
import PleyTesting
import Testing

@Suite("Downloads' answers decode as the Android app reads them")
struct DownloadsDecodingTests {
    @Test func theOriginal() throws {
        let info = try PleyJSON.decoder.decode(DownloadInfo.self, from: Fixtures.data("conversions/direct.json"))
        #expect(info.id == nil && info.state == .direct && info.progress == 1)
        #expect(info.size == 4_294_967_296 && info.url == "/api/files/601/stream")
    }

    @Test func aConversionUnderWay() throws {
        let info = try PleyJSON.decoder.decode(DownloadInfo.self, from: Fixtures.data("conversions/running.json"))
        #expect(info.id == "0123456789abcdef0123456789abcdef" && info.state == .running)
        #expect(info.progress == 0.42 && info.size == nil && info.error == nil)
        #expect(info.url == "/api/conversions/0123456789abcdef0123456789abcdef/file")
    }
}
