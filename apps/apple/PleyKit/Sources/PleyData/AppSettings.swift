import Foundation

/// This device's own settings, kept in `UserDefaults` under `pley.settings.*`:
/// nothing here goes to the server, and nothing is a secret. Each change is
/// kept at once.
public final class AppSettings: @unchecked Sendable {
    /// The key prefix every setting shares.
    public static let prefix = "pley.settings."

    private let defaults: UserDefaults

    /// Settings in `defaults` (the app's standard defaults by default).
    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// The section shown last (the tab or the sidebar's row), as the
    /// screens name it; `nil` before one was chosen.
    public var lastSection: String? {
        get { defaults.string(forKey: Self.prefix + "section") }
        set { set(newValue, for: "section") }
    }

    /// How a library was left on this device (its tab, sort and filters,
    /// as `LibraryParams` writes them), by server, profile and library.
    public func libraryView(origin: String, profileId: Int, libraryId: Int) -> String? {
        let views = defaults.dictionary(forKey: Self.prefix + "libraryViews") as? [String: String]
        return views?[Self.viewKey(origin, profileId, libraryId)]
    }

    public func setLibraryView(_ view: String, origin: String, profileId: Int, libraryId: Int) {
        var views = defaults.dictionary(forKey: Self.prefix + "libraryViews") as? [String: String] ?? [:]
        views[Self.viewKey(origin, profileId, libraryId)] = view
        // A bound: the oldest kept views don't matter much.
        if views.count > 500 { views = Dictionary(uniqueKeysWithValues: views.suffix(400).map { ($0.key, $0.value) }) }
        defaults.set(views, forKey: Self.prefix + "libraryViews")
    }

    private static func viewKey(_ origin: String, _ profileId: Int, _ libraryId: Int) -> String {
        "\(origin) \(profileId) \(libraryId)"
    }

    private func set(_ value: String?, for key: String) {
        if let value {
            defaults.set(value, forKey: Self.prefix + key)
        } else {
            defaults.removeObject(forKey: Self.prefix + key)
        }
    }
}
