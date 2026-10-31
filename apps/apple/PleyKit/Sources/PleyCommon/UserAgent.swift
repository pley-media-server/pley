import Foundation
#if canImport(UIKit)
import UIKit
#endif

/// How the app's requests introduce themselves:
/// `pleyApple/0.1.0 (iPadOS 16.7; iPad)`. The server names the app after
/// it in Signed-in apps ("Pley for iPad"), so it starts with `pleyApple/`
/// whatever else changes, and the operating system says which device it is.
/// The device part is the kind of device, never the name someone gave it,
/// so no personal data goes in the header.
public enum UserAgent {
    /// Long enough for any device kind; the same bound as the Android app's.
    static let maxName = 40

    public static func format(version: String, osName: String, osVersion: String, device: String) -> String {
        let device = cleaned(device)
        let system = "\(cleaned(osName)) \(cleaned(osVersion))"
        return "pleyApple/\(cleaned(version)) (\(device.isEmpty ? system : "\(system); \(device)"))"
    }

    /// This app's User-Agent. It asks UIKit what the device is, which only
    /// the main actor may.
    @MainActor
    public static var current: String {
        format(version: appVersion, osName: osName, osVersion: osVersion, device: deviceKind)
    }

    /// The app's version, `?` when there's no app bundle (the package's tests).
    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }

    /// `18.1`, or `18.1.2` when the patch isn't 0.
    static var osVersion: String {
        let v = ProcessInfo.processInfo.operatingSystemVersion
        return v.patchVersion == 0 ? "\(v.majorVersion).\(v.minorVersion)"
            : "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)"
    }

    @MainActor
    static var osName: String {
        #if os(tvOS)
        "tvOS"
        #elseif os(iOS)
        UIDevice.current.userInterfaceIdiom == .pad ? "iPadOS" : "iOS"
        #elseif os(macOS)
        "macOS"
        #else
        "?"
        #endif
    }

    /// "iPhone", "iPad", "Apple TV" or "Mac".
    @MainActor
    static var deviceKind: String {
        #if os(iOS) || os(tvOS)
        UIDevice.current.model
        #else
        "Mac"
        #endif
    }

    /// `name` as a header carries it: printable ASCII (accents dropped,
    /// typographic quotes and dashes made plain, the rest left out), without
    /// the brackets, semicolons and backslashes the User-Agent is made of,
    /// at most 40 characters.
    public static func cleaned(_ name: String) -> String {
        var out = ""
        for scalar in name.decomposedStringWithCanonicalMapping.unicodeScalars {
            if scalar.properties.generalCategory == .nonspacingMark { continue }
            switch scalar {
            case "\u{2018}", "\u{2019}", "\u{201B}", "\u{2032}": out.append("'")
            case "\u{201C}", "\u{201D}", "\u{2033}": out.append("\"")
            case "\u{2013}", "\u{2014}", "\u{2010}", "\u{2011}": out.append("-")
            case " "..."~": out.append("();\\".unicodeScalars.contains(scalar) ? " " : Character(scalar))
            default: out.append(" ")
            }
        }
        let words = out.split(separator: " ").joined(separator: " ")
        return String(words.prefix(maxName)).trimmingCharacters(in: .whitespaces)
    }
}
