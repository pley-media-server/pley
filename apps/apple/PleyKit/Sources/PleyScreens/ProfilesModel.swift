import Combine
import Foundation
import PleyAuth
import PleyModel

/// Choosing who is watching: the household's profiles, and moving the
/// sign-in to one, with its PIN when it has one.
@MainActor
public final class ProfilesModel: ObservableObject {
    @Published public private(set) var profiles: [HouseholdProfile] = []
    @Published public private(set) var loading = false
    /// The profile whose PIN is being asked for.
    @Published public var pinFor: HouseholdProfile?
    @Published public var pin = ""
    /// Why the PIN wasn't taken, under the PIN field.
    @Published public private(set) var pinError: String?
    @Published public var error: String?
    /// The profile being switched to.
    @Published public private(set) var switching: Int?

    public let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    /// Whether `profile` can be chosen here: one locked by the server can't.
    public static func canChoose(_ profile: HouseholdProfile) -> Bool {
        profile.needs != .locked
    }

    public func load() async {
        guard let switcher = await app.session.profiles else { return }
        loading = profiles.isEmpty
        defer { loading = false }
        do {
            profiles = try await switcher.profiles()
            error = nil
        } catch is CancellationError {
        } catch {
            self.error = describe(error)
        }
    }

    /// Chooses `profile`: the one in use goes back to Home, one with a PIN
    /// asks for it, one that needs its password says how to use it.
    public func choose(_ profile: HouseholdProfile) async {
        error = nil
        if profile.current {
            app.keepProfile()
            return
        }
        switch profile.needs {
        case .none:
            await switchTo(profile, pin: nil)
        case .pin:
            pin = ""
            pinError = nil
            pinFor = profile
        case .password:
            error = ProfileSwitchError.needsBrowser.message
        default:
            // Locked, or something this app doesn't know: the server's page decides.
            error = ProfileSwitchError.notAllowed.message
        }
    }

    /// Sends the PIN typed for `pinFor`; one that isn't 4 to 8 digits isn't sent.
    public func submitPin() async {
        guard let profile = pinFor else { return }
        guard ProfileSwitcher.isPin(pin) else {
            pinError = ProfileSwitchError.pinFormat.message
            return
        }
        await switchTo(profile, pin: pin)
    }

    public func cancelPin() {
        pinFor = nil
        pin = ""
        pinError = nil
    }

    private func switchTo(_ profile: HouseholdProfile, pin: String?) async {
        guard switching == nil, let switcher = await app.session.profiles else { return }
        switching = profile.id
        defer { switching = nil }
        // The playback open belongs to the profile before: it ends first, reported as that profile.
        await app.closePlayer()
        do {
            try await switcher.switchTo(profile.id, pin: pin)
            cancelPin()
            await app.profileChanged()
        } catch is CancellationError {
        } catch {
            let text = describe(error)
            if pin != nil {
                pinError = text
                self.pin = ""
            } else {
                self.error = text
            }
        }
    }
}

/// How a profile looks without its picture.
public enum Avatar {
    /// Up to two initials: `Alex Smith` → `AS`.
    public static func initials(_ name: String) -> String {
        let words = name.split(whereSeparator: { $0.isWhitespace || $0 == "-" })
        let letters = words.prefix(2).compactMap { $0.first.map { String($0).uppercased() } }
        return letters.isEmpty ? "?" : letters.joined()
    }

    /// `#rrggbb` (or `#rgb`) as red, green and blue from 0 to 1; `nil` for anything else.
    public static func rgb(_ hex: String?) -> (red: Double, green: Double, blue: Double)? {
        guard var text = hex?.trimmingCharacters(in: .whitespaces), text.hasPrefix("#") else { return nil }
        text.removeFirst()
        if text.count == 3 { text = text.map { "\($0)\($0)" }.joined() }
        guard text.count == 6, text.allSatisfy(\.isHexDigit), let value = UInt32(text, radix: 16) else { return nil }
        return (Double((value >> 16) & 0xff) / 255, Double((value >> 8) & 0xff) / 255, Double(value & 0xff) / 255)
    }
}
