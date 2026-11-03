// The menus are UIKit's: these run on the iOS and tvOS simulators
// (`xcodebuild test -scheme PleyKit-Package`), and are empty on the Mac.
#if os(iOS) || os(tvOS)
import PleyData
import PleyPlayer
@testable import PleyPlayerViews
import Testing
import UIKit

@MainActor
@Suite struct PlayerMenusTests {
    private var qualities: [MenuChoice] {
        [MenuChoice(id: MenuChoice.original, label: "Original", detail: "The file as it is", selected: true),
         MenuChoice(id: "auto", label: "Automatic", detail: "Up to 1080p"),
         MenuChoice(id: "720p", label: "720p", detail: "4 Mbps")]
    }

    @Test func theCheckedQualityKeepsItsSecondLine() throws {
        var picked: [String] = []
        var actions = PlayerMenus.Actions()
        actions.quality = { picked.append($0) }
        let menu = PlayerMenus.quality(qualities, actions)
        let rows = try #require(menu.children as? [UIAction])
        #expect(rows.map(\.title) == ["Original", "Automatic", "720p"])
        #expect(rows.allSatisfy { $0.subtitle != nil })
        // No row is `.on`: a checked row would lose its second line.
        #expect(rows.allSatisfy { $0.state == .off })
        #expect(rows[0].image != nil)
        #expect(rows[1].image == nil && rows[2].image == nil)
        #expect(rows[0].accessibilityLabel == "Original, The file as it is, selected")
        #expect(rows[1].accessibilityLabel == nil)
    }

    @Test func audioAndSubtitlesAreCheckedTheSameWay() throws {
        var state = PlayerState()
        state.audio = [MenuChoice(id: "1", label: "English (AAC Stereo)", selected: true),
                       MenuChoice(id: "2", label: "French (AC3 5.1)")]
        state.subtitles = [MenuChoice(id: MenuChoice.off, label: "Off"),
                           MenuChoice(id: "3", label: "English (SRT)", detail: "Drawn by Pley", selected: true)]
        var subtitle: [Int?] = []
        var actions = PlayerMenus.Actions()
        actions.subtitle = { subtitle.append($0) }
        let audio = try #require(PlayerMenus.audio(state.audio, actions).children as? [UIAction])
        #expect(audio.allSatisfy { $0.state == .off })
        #expect(audio.map { $0.image != nil } == [true, false])
        let menu = PlayerMenus.subtitles(state, actions)
        let tracks = try #require((menu.children.first as? UIMenu)?.children as? [UIAction])
        #expect(tracks.allSatisfy { $0.state == .off })
        #expect(tracks[1].subtitle == "Drawn by Pley")
        #expect(tracks.map { $0.image != nil } == [false, true])
    }

    @Test func speedChecksTheOnePlayingAndSaysItBeside() throws {
        var state = PlayerState()
        state.speed = 1.5
        var picked: [Double] = []
        var actions = PlayerMenus.Actions()
        actions.speed = { picked.append($0) }
        let menu = PlayerMenus.speed(state, actions)
        #expect(menu.title == "Speed")
        #expect(menu.subtitle == "1.5×")
        let rows = try #require(menu.children as? [UIAction])
        #expect(rows.map(\.title) == ["0.5×", "0.75×", "1×", "1.25×", "1.5×", "2×"])
        #expect(rows.map { $0.image != nil } == [false, false, false, false, true, false])
    }

    @Test func theSleepTimerSaysWhatIsLeft() throws {
        var state = PlayerState()
        state.sleepItem = "episode"
        state.sleep = SleepState(choice: .minutes(15), minutesLeft: 12)
        let menu = PlayerMenus.sleepTimer(state, PlayerMenus.Actions())
        #expect(menu.title == "Sleep Timer")
        #expect(menu.subtitle == "12 min left")
        let rows = try #require(menu.children as? [UIAction])
        #expect(rows.first?.title == "Off")
        #expect(rows.last?.title == "End of this episode")
        #expect(rows.filter { $0.image != nil }.map(\.title) == ["15 minutes"])
    }

    @Test func findSubtitlesIsLastInSubtitlesWhileItStreams() throws {
        var state = PlayerState()
        state.subtitles = [MenuChoice(id: MenuChoice.off, label: "Off", selected: true)]
        var opened = 0
        var actions = PlayerMenus.Actions()
        actions.findSubtitles = { opened += 1 }
        // Not streamed from the server (or nothing playing yet): no search.
        #expect(PlayerMenus.findSubtitles(state, actions) == nil)
        state.canFindSubtitles = true
        let menu = PlayerMenus.subtitles(state, actions)
        let find = try #require(menu.children.last as? UIAction)
        #expect(find.title == "Find Subtitles…")
        // Without a screen to open the search, it isn't offered.
        #expect(PlayerMenus.findSubtitles(state, PlayerMenus.Actions()) == nil)
    }

    @Test func theStyleIsInSubtitlesWhileTextSubtitlesShow() throws {
        var state = PlayerState()
        state.subtitles = [MenuChoice(id: MenuChoice.off, label: "Off"), MenuChoice(id: "2", label: "English",
                                                                                     selected: true)]
        state.subtitleStyle = SubtitleOverrides(size: .p125)
        let actions = PlayerMenus.Actions()
        // Pictures, or Off: no style.
        #expect(PlayerMenus.subtitleStyle(state, actions) == nil)
        state.textSubtitles = true
        let style = try #require(PlayerMenus.subtitleStyle(state, actions))
        #expect((PlayerMenus.subtitles(state, actions).children.last as? UIMenu)?.title == "Subtitle style")
        let menus = style.children.compactMap { $0 as? UIMenu }
        #expect(menus.map(\.title) == ["Size", "Background", "Font", "Position"])
        #expect(menus.map(\.subtitle) == ["125 %", "System", "System", "Bottom"])
        let sizes = try #require(menus.first?.children as? [UIAction])
        #expect(sizes.filter { $0.image != nil }.map(\.title) == ["125 %"])
        let position = try #require(menus.last?.children as? [UIAction])
        #expect(position.map(\.title) == ["Bottom", "Raised 5 %", "Raised 10 %", "Raised 15 %"])
    }

    #if os(iOS)
    @Test func theOptionsMenuHoldsEverySection() {
        var state = PlayerState()
        state.qualities = qualities
        state.audio = [MenuChoice(id: "1", label: "English", selected: true), MenuChoice(id: "2", label: "French")]
        state.subtitles = [MenuChoice(id: MenuChoice.off, label: "Off", selected: true)]
        state.chapters = [ChapterChoice(index: 0, title: "Prologue", startMs: 0, current: true)]
        let menu = PlayerMenus.options(state, PlayerMenus.Actions())
        let titles = menu.children.compactMap { ($0 as? UIMenu)?.title }
        #expect(titles == ["Quality", "Audio", "Subtitles", "Speed", "Sleep Timer", "Chapters", "Details"])
        // Text subtitles add their style after them.
        state.textSubtitles = true
        let styled = PlayerMenus.options(state, PlayerMenus.Actions()).children.compactMap { ($0 as? UIMenu)?.title }
        #expect(styled == ["Quality", "Audio", "Subtitles", "Subtitle style", "Speed", "Sleep Timer", "Chapters",
                           "Details"])
        let quality = menu.children.first as? UIMenu
        #expect(quality?.options.contains(.displayInline) == true)
        #expect((quality?.children as? [UIAction])?.allSatisfy { $0.subtitle != nil && $0.state == .off } == true)
    }
    #endif
}
#endif
