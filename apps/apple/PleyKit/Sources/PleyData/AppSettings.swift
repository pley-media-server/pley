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

    private func set(_ value: String?, for key: String) {
        if let value {
            defaults.set(value, forKey: Self.prefix + key)
        } else {
            defaults.removeObject(forKey: Self.prefix + key)
        }
    }
}
