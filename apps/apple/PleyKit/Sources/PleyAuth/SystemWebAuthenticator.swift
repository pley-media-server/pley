#if os(iOS) || os(macOS)
import AuthenticationServices
import Foundation

#if os(iOS)
import UIKit
#else
import AppKit
#endif

/// The system's sign-in sheet (`ASWebAuthenticationSession`): Safari's
/// engine, outside the app, which catches the return address itself, so the
/// app registers no URL scheme.
///
/// The session is ephemeral by default, as the Android app's private tab:
/// it shares no cookie with Safari and keeps none afterwards, so no Pley
/// cookie with the account's full rights stays on the device, and the
/// password is typed at every sign-in. Settings' "Sign in privately" off
/// shares Safari's cookies instead (`ephemeral` says which, at each sign-in).
public struct SystemWebAuthenticator: WebAuthenticator {
    private let ephemeral: @Sendable () -> Bool

    public init(ephemeral: @escaping @Sendable () -> Bool = { true }) {
        self.ephemeral = ephemeral
    }

    public func authenticate(url: URL, callbackScheme: String) async throws -> URL {
        try await Self.run(url: url, callbackScheme: callbackScheme, ephemeral: ephemeral())
    }

    @MainActor
    private static func run(url: URL, callbackScheme: String, ephemeral: Bool) async throws -> URL {
        let sheet = Sheet()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                sheet.start(url: url, callbackScheme: callbackScheme, ephemeral: ephemeral, continuation: continuation)
            }
        } onCancel: {
            Task { @MainActor in sheet.cancel() }
        }
    }
}

/// One sign-in sheet, kept alive until it answers.
@MainActor
private final class Sheet: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, callbackScheme: String, ephemeral: Bool,
               continuation: CheckedContinuation<URL, any Error>) {
        // The iOS 16 initialiser: the `Callback` one needs iOS 17.4. The
        // handler is `@Sendable`, not the main actor's: macOS calls it on a
        // queue of its own, where a main-actor closure stops the app.
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { @Sendable callback, error in
            if let callback {
                continuation.resume(returning: callback)
            } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                continuation.resume(throwing: AuthError.cancelled)
            } else {
                continuation.resume(throwing: AuthError.noBrowser)
            }
            Task { @MainActor in self.session = nil }
        }
        session.prefersEphemeralWebBrowserSession = ephemeral
        session.presentationContextProvider = self
        self.session = session
        if !session.start() {
            self.session = nil
            continuation.resume(throwing: AuthError.noBrowser)
        }
    }

    func cancel() {
        session?.cancel()
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated { Self.keyWindow() }
    }

    private static func keyWindow() -> ASPresentationAnchor {
        #if os(iOS)
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap(\.windows)
        return windows.first(where: \.isKeyWindow) ?? windows.first ?? ASPresentationAnchor()
        #else
        return NSApplication.shared.keyWindow ?? NSApplication.shared.windows.first ?? ASPresentationAnchor()
        #endif
    }
}
#endif
