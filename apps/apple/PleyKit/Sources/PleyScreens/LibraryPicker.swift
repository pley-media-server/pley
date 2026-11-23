import Foundation
import PleyModel

/// A place the iPhone's Library tab can show, as its picker lists it.
public struct LibraryPickerEntry: Identifiable, Hashable, Sendable {
    /// What choosing it shows: a library's section, or Live TV's.
    public let section: ShellSection
    public let title: String
    /// "412 movies", "106 channels".
    public let detail: String
    /// The SF Symbol of its kind.
    public let symbol: String

    public var id: String { section.key }
    /// Live TV, marked as live.
    public var live: Bool { section == .liveTv }
    /// What VoiceOver says: "Movies, 412 movies".
    public var spoken: String { [title, detail].filter { !$0.isEmpty }.joined(separator: ", ") }
}

/// The iPhone's Library tab (and the iPad's in a narrow window), which
/// takes the place of Libraries: a tap opens a picker of the profile's
/// movie and show libraries, in its order, each with what it holds (music
/// is under Listen, `ListenModel`), then Live TV with
/// its channels when it shows, then Edit order (the libraries' list,
/// where they are put in order). Choosing one shows it under the tab,
/// whose label takes its name; until one is chosen, it is "Library". With
/// one place to show, a tap goes straight there. The iPad's and the Mac's
/// sidebar list the libraries themselves, and have no picker.
public enum LibraryPicker {
    /// The places the picker lists, in order: no music library, which Listen has.
    public static func entries(libraries: [Library], liveTv: LiveSummary?) -> [LibraryPickerEntry] {
        var out = libraries.filter { $0.kind != .music }.map { library in
            LibraryPickerEntry(section: .library(library.id), title: library.name, detail: Words.libraryCount(library),
                               symbol: LibraryEntry.symbol(library.kind))
        }
        if let liveTv, liveTv.available {
            out.append(LibraryPickerEntry(section: .liveTv, title: localized("Live TV"),
                                          detail: Words.channelCount(liveTv.channels), symbol: "tv"))
        }
        return out
    }

    /// Where a tap on the tab goes without the picker: the one place there
    /// is to show; `nil` when there are several (or none), and the picker opens.
    public static func direct(_ entries: [LibraryPickerEntry]) -> ShellSection? {
        entries.count == 1 ? entries[0].section : nil
    }

    /// Whether `section` shows under the Library tab.
    public static func underTab(_ section: ShellSection) -> Bool {
        switch section {
        case .libraries, .library, .liveTv: true
        default: false
        }
    }

    /// The tab's label: the place showing under it, else the one shown
    /// last, by name and symbol; "Library" before any.
    public static func label(showing: ShellSection, last: ShellSection?,
                             libraries: [Library]) -> (title: String, symbol: String) {
        let place = underTab(showing) && showing != .libraries ? showing : last
        switch place {
        case .library(let id)?:
            if let library = libraries.first(where: { $0.id == id }) {
                return (library.name, LibraryEntry.symbol(library.kind))
            }
        case .liveTv?:
            return (localized("Live TV"), "tv")
        default:
            break
        }
        return (localized("Library"), "square.stack")
    }
}
