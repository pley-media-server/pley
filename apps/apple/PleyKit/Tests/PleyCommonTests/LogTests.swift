import PleyCommon
import Testing

@Suite struct LogTests {
    @Test func keyedMediaAddressesAreLoggedWithoutTheirKey() {
        let logged = Log.path("/api/media/pley_mk_x/master.m3u8")
        #expect(!logged.contains("pley_mk_x"))
        #expect(logged == "/api/media/…")
        #expect(!Log.path("/API/Media/pley_mk_x/0.m4s").contains("pley_mk_x"))
    }

    @Test func otherPathsAreLoggedAsTheyAre() {
        #expect(Log.path("/api/items/42") == "/api/items/42")
        #expect(Log.path("/api/playback/progress") == "/api/playback/progress")
    }
}
