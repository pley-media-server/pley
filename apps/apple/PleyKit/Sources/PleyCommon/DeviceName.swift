import Foundation
#if canImport(UIKit)
import UIKit
#endif
#if os(macOS)
import SystemConfiguration
#endif

/// What the profile's remotes call this device: the name someone gave it
/// ("Living room", "Alex's MacBook Air"), sent with a play request so the
/// remote on their other devices says where it plays. It goes in the
/// request's body to the server only, never in a header.
public enum DeviceName {
    /// The server's bound for a device's name.
    static let maxLength = 64

    /// The Mac's computer name (System Settings → General → Sharing). On
    /// iPhone, iPad and Apple TV, the name someone gave the device, which
    /// the system tells only to apps it allows: others get the kind of
    /// device ("iPhone"), which says no more than the server's own name
    /// from the User-Agent ("Pley for iPhone"), so `nil` then.
    @MainActor
    public static var current: String? {
        #if os(macOS)
        let name = SCDynamicStoreCopyComputerName(nil, nil) as String?
        return name.flatMap(cleaned)
        #elseif os(iOS) || os(tvOS)
        let device = UIDevice.current
        return device.name == device.model ? nil : cleaned(device.name)
        #else
        return nil
        #endif
    }

    /// What kind of device this is, as the server names kinds
    /// (`device_kind`): `tv` (Apple TV), `tablet` (iPad), `phone`
    /// (iPhone) or `computer` (Mac).
    @MainActor
    public static var kind: String {
        #if os(tvOS)
        "tv"
        #elseif os(iOS)
        UIDevice.current.userInterfaceIdiom == .pad ? "tablet" : "phone"
        #elseif os(macOS)
        "computer"
        #else
        "other"
        #endif
    }

    /// `name` trimmed, at most 64 characters; `nil` when nothing is left.
    public static func cleaned(_ name: String) -> String? {
        let trimmed = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(maxLength))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
