// The Mac player's action menu: AppKit, so these run on the Mac only.
#if os(macOS)
import AppKit
import PleyPlayer
@testable import PleyPlayerViews
import Testing

@MainActor
@Suite struct ActionMenuTests {
    private func state(quality: String, audio: Int = 2) -> PlayerState {
        var state = PlayerState()
        state.qualities = [MenuChoice(id: MenuChoice.original, label: "Original", detail: "The file as it is",
                                      selected: quality == MenuChoice.original),
                           MenuChoice(id: "720p", label: "720p", detail: "4 Mbps", selected: quality == "720p")]
        state.audio = (1...audio).map { MenuChoice(id: "\($0)", label: "Track \($0)", selected: $0 == 1) }
        return state
    }

    @Test func theMenuIsFilledFromTheStateEachTimeItOpens() throws {
        var now = state(quality: MenuChoice.original)
        let source = ActionMenuSource(state: { now })
        let menu = NSMenu()
        menu.delegate = source

        source.menuNeedsUpdate(menu)
        #expect(menu.items.map(\.title) == ["Quality", "Audio", "Details"])
        var quality = try #require(menu.items.first?.submenu)
        #expect(quality.items.map(\.state) == [.on, .off])
        #expect(quality.items.map(\.subtitle) == ["The file as it is", "4 Mbps"])

        // The state moves on while it plays; the menu is made again only as it opens.
        now = state(quality: "720p", audio: 1)
        #expect(menu.items.map(\.title) == ["Quality", "Audio", "Details"])
        source.menuNeedsUpdate(menu)
        #expect(menu.items.map(\.title) == ["Quality", "Details"])
        quality = try #require(menu.items.first?.submenu)
        #expect(quality.items.map(\.state) == [.off, .on])
    }

    @Test func withoutAStateTheMenuIsEmpty() {
        let source = ActionMenuSource(state: { nil })
        let menu = NSMenu()
        menu.addItem(NSMenuItem(title: "Left over", action: nil, keyEquivalent: ""))
        source.menuNeedsUpdate(menu)
        #expect(menu.items.isEmpty)
    }

    @Test func thePlayerViewSetsItsMenuOnce() {
        let view = PleyPlayerView()
        view.addActionMenu()
        let menu = view.actionPopUpButtonMenu
        #expect(menu?.delegate === view.actionMenuSource)
        // Its items wait for the menu to open; opening it again fills the same menu.
        #expect(menu?.items.isEmpty == true)
        view.actionMenuSource.state = { self.state(quality: "720p") }
        if let menu { view.actionMenuSource.menuNeedsUpdate(menu) }
        #expect(view.actionPopUpButtonMenu === menu)
        #expect(menu?.items.first?.title == "Quality")
    }
}
#endif
