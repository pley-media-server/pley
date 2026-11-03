import Foundation
@testable import PleyPlayerViews
import Testing

/// The subtitle view over the video, as it follows the caption style.
@MainActor
@Suite struct SubtitleViewTests {
    /// A framework in the app (the system loads some by itself) may change
    /// its defaults on another thread, which tells the view on that thread:
    /// it goes on, and draws again on the main actor.
    @Test func defaultsChangedOnAnotherThreadAreHeardThere() async {
        let view = SubtitleView(frame: CGRect(x: 0, y: 0, width: 320, height: 180))
        await Task.detached {
            NotificationCenter.default.post(name: UserDefaults.didChangeNotification, object: nil)
        }.value
        await Task.yield()
        #expect(view.shown == .none)
    }
}
