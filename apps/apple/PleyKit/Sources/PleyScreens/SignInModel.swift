import Combine
import CoreGraphics
import Foundation
import PleyAuth
import PleyCommon
import PleyModel

/// Signing in to the open server: in the system's sign-in sheet on iPhone,
/// iPad and Mac, with a code on Apple TV (and on iPhone and iPad when
/// asked).
@MainActor
public final class SignInModel: ObservableObject {
    public enum Phase {
        case idle
        /// The sign-in sheet is open.
        case browser
        /// A code to show, with its QR code.
        case code(DeviceCode, qr: CGImage?)
        /// Asking the server.
        case working
        case failed(String)
    }

    @Published public private(set) var phase: Phase = .idle
    /// Seconds until the code shown runs out.
    @Published public private(set) var secondsLeft = 0
    /// Whether the last try was with a code, so a failure offers a new code.
    @Published public private(set) var usingCode = false

    public let app: AppModel
    public let entry: ServerEntry
    /// Why sign-in shows by itself (the last one ended), when it does.
    public let notice: String?
    /// The server is reached over plain http at an address that isn't
    /// private: the server's page warns, and the app refuses to sign in.
    public let unencryptedWarning: Bool

    private var task: Task<Void, Never>?
    private var countdown: Task<Void, Never>?

    public init(app: AppModel, entry: ServerEntry) {
        self.app = app
        self.entry = entry
        notice = app.notice
        unencryptedWarning = Servers.unencrypted(entry.origin)
    }

    public var canUseBrowser: Bool { app.client.signsInWithBrowser && app.authenticator != nil }
    public var canUseCode: Bool { app.client.signsInWithCode }

    /// The address shown with the code (`http://host:port/link`), without the code in it.
    public var codeAddress: String? {
        guard case .code(let code, _) = phase else { return nil }
        return code.verificationURL.absoluteString
    }

    public func signInWithBrowser() {
        guard canUseBrowser, let authenticator = app.authenticator else { return }
        run(code: false) { [app] model in
            model.phase = .browser
            let user = try await app.session.signInWithBrowser(authenticator: authenticator)
            model.phase = .working
            return user
        }
    }

    /// Asks for a code, shows it and waits until it is allowed; a new call
    /// gets a new code.
    public func signInWithCode() {
        guard canUseCode else { return }
        run(code: true) { [app] model in
            model.phase = .working
            let signIn = try await app.session.deviceSignIn()
            let code = try await signIn.start()
            model.show(code)
            _ = try await signIn.waitForApproval(code)
            model.stopCountdown()
            model.phase = .working
            return try await app.session.completeSignIn()
        }
    }

    /// Stops waiting for the sheet or the code.
    public func cancel() {
        task?.cancel()
        task = nil
        stopCountdown()
        phase = .idle
    }

    // MARK: Inside

    private func run(code: Bool, _ body: @escaping @MainActor (SignInModel) async throws -> User) {
        task?.cancel()
        stopCountdown()
        usingCode = code
        app.notice = nil
        task = Task { [weak self, app] in
            guard let self else { return }
            do {
                let user = try await body(self)
                try Task.checkCancellation()
                await app.signedIn(user)
                self.phase = .idle
            } catch AuthError.cancelled {
                self.phase = .idle
            } catch is CancellationError {
                return
            } catch {
                if Task.isCancelled { return }
                self.stopCountdown()
                self.phase = .failed(describe(error))
            }
        }
    }

    private func show(_ code: DeviceCode) {
        let link = code.completeURL ?? code.verificationURL
        phase = .code(code, qr: QRCode.make(link.absoluteString))
        stopCountdown()
        secondsLeft = Self.seconds(until: code.expiresAt)
        countdown = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !Task.isCancelled else { return }
                self.secondsLeft = Self.seconds(until: code.expiresAt)
            }
        }
    }

    private func stopCountdown() {
        countdown?.cancel()
        countdown = nil
    }

    static func seconds(until date: Date) -> Int {
        max(0, Int(date.timeIntervalSinceNow.rounded(.up)))
    }

    /// `seconds` as people read a countdown: `9:05`.
    public static func minutes(_ seconds: Int) -> String {
        let seconds = max(0, seconds)
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}
