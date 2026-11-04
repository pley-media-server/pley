// Serves the package's fixture server (PleyTesting's FixtureServer) on
// 127.0.0.1 for the UI tests of the screens a real test server can't show:
// requests, whose titles come from TMDB, which the test server has no key
// for. scripts/check-screens.sh runs it beside each Apple TV walk.
//
//   pley-apple-fixtures
//
// It prints its origin on the first line, then serves until it is stopped.
// The household is the fixture's (Alex, Sam with a PIN, Kids, Robin,
// Guest), its sign-in codes are allowed as soon as they are asked for, and
// Alex is an administrator who approves and declines, with one request
// ready to watch (Arrival) that the account hasn't been told of yet.
// Requests and the decisions sent are printed as they come, without any
// token.

#if os(macOS)
import Foundation
import PleyTesting

let server = FixtureServer()
server.approveDevice()
server.useManagerSummary()
server.setUnseenRequests([45])
let origin = try await server.start()
print(origin)
fflush(stdout)

// What the app asked for, a line each, for the walk's log.
var told = 0
while true {
    try await Task.sleep(for: .milliseconds(500))
    let journal = server.journal
    for request in journal.dropFirst(told) {
        print("\(request.method) \(request.path)")
    }
    told = journal.count
    fflush(stdout)
}
#else
// Only the Mac serves it; elsewhere the package still builds.
print("pley-apple-fixtures runs on macOS only")
#endif
