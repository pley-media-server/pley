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
/// The session is ephemeral, as the Android app's private tab: it shares no
/// cookie with Safari and keeps none afterwards, so no Pley cookie with the
/// account's full rights stays on the device. The password is typed at
/// every sign-in.
public struct SystemWebAuthenticator: WebAuthenticator {
    public init() {}

    public func authenticate(url: URL, callbackScheme: String) async throws -> URL {
        try await Self.run(url: url, callbackScheme: callbackScheme)
    }

    @MainActor
    private static func run(url: URL, callbackScheme: String) async throws -> URL {
        let sheet = Sheet()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                sheet.start(url: url, callbackScheme: callbackScheme, continuation: continuation)
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

    func start(url: URL, callbackScheme: String, continuation: CheckedContinuation<URL, any Error>) {
        // The iOS 16 initialiser: the `Callback` one needs iOS 17.4.
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { callback, error in
            if let callback {
                continuation.resume(returning: callback)
            } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                continuation.resume(throwing: AuthError.cancelled)
            } else {
                continuation.resume(throwing: AuthError.noBrowser)
            }
            Task { @MainActor in self.session = nil }
        }
        session.prefersEphemeralWebBrowserSession = true
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
