// The menus are UIKit's: these run on the iOS and tvOS simulators
// (`xcodebuild test -scheme PleyKit-Package`), and are empty on the Mac.
#if os(iOS) || os(tvOS)
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

    #if os(iOS)
    @Test func theOptionsMenuHoldsEverySection() {
        var state = PlayerState()
        state.qualities = qualities
        state.audio = [MenuChoice(id: "1", label: "English", selected: true), MenuChoice(id: "2", label: "French")]
        state.subtitles = [MenuChoice(id: MenuChoice.off, label: "Off", selected: true)]
        state.chapters = [ChapterChoice(index: 0, title: "Prologue", startMs: 0, current: true)]
        let menu = PlayerMenus.options(state, PlayerMenus.Actions())
        let titles = menu.children.compactMap { ($0 as? UIMenu)?.title }
        #expect(titles == ["Quality", "Audio", "Subtitles", "Chapters", "Details"])
        let quality = menu.children.first as? UIMenu
        #expect(quality?.options.contains(.displayInline) == true)
        #expect((quality?.children as? [UIAction])?.allSatisfy { $0.subtitle != nil && $0.state == .off } == true)
    }
    #endif
}
#endif
