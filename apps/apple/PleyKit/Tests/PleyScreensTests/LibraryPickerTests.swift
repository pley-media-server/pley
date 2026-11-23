import Foundation
import PleyModel
@testable import PleyScreens
import Testing

/// The iPhone's Library tab: its picker's places, when it opens, and the
/// tab's label.
@MainActor
@Suite struct LibraryPickerTests {
    private let libraries = [Library(id: 1, name: "Movies", kind: .movies, itemCount: 412),
                             Library(id: 2, name: "TV Shows", kind: .shows, itemCount: 38),
                             Library(id: 3, name: "Music", kind: .music, itemCount: 120)]

    @Test func thePickerListsTheLibrariesInOrderThenLiveTV() {
        let live = LiveSummary(available: true, allowed: true, channels: 106)
        let entries = LibraryPicker.entries(libraries: libraries, liveTv: live)
        // Music is under Listen.
        #expect(entries.map(\.section) == [.library(1), .library(2), .liveTv])
        #expect(entries.map(\.detail) == ["412 movies", "38 shows", "106 channels"])
        #expect(entries.map(\.symbol) == ["film", "tv", "tv"])
        #expect(entries.map(\.live) == [false, false, true])
        #expect(entries[0].spoken == "Movies, 412 movies")
        #expect(LibraryPicker.direct(entries) == nil)
        // Live TV only when it shows.
        #expect(LibraryPicker.entries(libraries: libraries, liveTv: LiveSummary(available: false, allowed: true))
            .map(\.section) == [.library(1), .library(2)])
        #expect(LibraryPicker.entries(libraries: libraries, liveTv: nil).count == 2)
    }

    @Test func onePlaceToShowIsGoneToWithoutThePicker() {
        let one = LibraryPicker.entries(libraries: [libraries[0]], liveTv: nil)
        #expect(LibraryPicker.direct(one) == .library(1))
        let live = LibraryPicker.entries(libraries: [], liveTv: LiveSummary(available: true, allowed: true, channels: 1))
        #expect(LibraryPicker.direct(live) == .liveTv)
        #expect(live[0].detail == "1 channel")
        #expect(LibraryPicker.direct(LibraryPicker.entries(libraries: [], liveTv: nil)) == nil)
    }

    @Test func theTabTakesTheNameOfThePlaceShowingOrShownLast() {
        #expect(LibraryPicker.label(showing: .home, last: nil, libraries: libraries) == ("Library", "square.stack"))
        #expect(LibraryPicker.label(showing: .library(2), last: nil, libraries: libraries) == ("TV Shows", "tv"))
        #expect(LibraryPicker.label(showing: .search, last: .library(1), libraries: libraries) == ("Movies", "film"))
        #expect(LibraryPicker.label(showing: .liveTv, last: .library(1), libraries: libraries) == ("Live TV", "tv"))
        // The list of libraries (Edit order) keeps the name of the one shown last.
        #expect(LibraryPicker.label(showing: .libraries, last: .library(2), libraries: libraries).title == "TV Shows")
        // A library gone since: "Library" again.
        #expect(LibraryPicker.label(showing: .home, last: .library(9), libraries: libraries).title == "Library")
    }

    @Test func theShellRemembersTheLibraryShownLast() {
        let shell = ShellModel(settings: freshSettings(), tv: false)
        shell.setLibraries(libraries)
        #expect(shell.lastLibrary == nil)
        shell.select(.library(2))
        #expect(shell.lastLibrary == .library(2))
        shell.select(.home)
        #expect(shell.lastLibrary == .library(2))
        shell.select(.libraries)
        #expect(shell.lastLibrary == .library(2))
    }
}
