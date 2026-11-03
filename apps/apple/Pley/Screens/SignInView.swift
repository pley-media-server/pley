import PleyAuth
import PleyCommon
import PleyNetwork
import PleyScreens
import SwiftUI

/// Signing in to a server: in the browser on iPhone, iPad and Mac, or on
/// iPhone and iPad with a code allowed on another device.
struct SignInView: View {
    @StateObject private var model: SignInModel

    init(app: AppModel, entry: ServerEntry) {
        _model = StateObject(wrappedValue: SignInModel(app: app, entry: entry))
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                VStack(spacing: 6) {
                    Text(model.entry.name)
                        .font(.title.bold())
                    Text(Servers.displayAddress(model.entry.origin))
                        .foregroundStyle(.secondary)
                }
                .multilineTextAlignment(.center)
                if let notice = model.notice {
                    Label(asSentence(notice), systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                }
                if model.unencryptedWarning {
                    Label(asSentence(PleyError.unencryptedMessage), systemImage: "lock.open")
                        .foregroundStyle(.orange)
                }
                content
            }
            .padding(24)
            .frame(maxWidth: 520)
            .frame(maxWidth: .infinity)
        }
        .navigationTitle("Sign in")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .onDisappear { model.cancel() }
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .code(let code, let qr):
            CodeView(code: code, qr: qr, secondsLeft: model.secondsLeft) { model.cancel() }
        case .browser, .working:
            ProgressView()
                .controlSize(.large)
                .padding()
        case .idle, .failed:
            VStack(spacing: 12) {
                if case .failed(let why) = model.phase {
                    Text(asSentence(why))
                        .foregroundStyle(.red)
                        .multilineTextAlignment(.center)
                }
                if model.canUseBrowser {
                    Button {
                        model.signInWithBrowser()
                    } label: {
                        Text("Sign in with your browser").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("signIn.browser")
                    .controlSize(.large)
                }
                if model.canUseCode {
                    Button {
                        model.signInWithCode()
                    } label: {
                        Text(model.usingCode ? "Get a new code" : "Sign in with a code").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("signIn.code")
                    .controlSize(.large)
                }
            }
        }
    }
}

/// A code to type on another device, with its QR code: the same as Apple TV shows.
private struct CodeView: View {
    let code: DeviceCode
    let qr: CGImage?
    let secondsLeft: Int
    let cancel: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            if let qr {
                Image(decorative: qr, scale: 1)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 220, height: 220)
                    .accessibilityLabel("QR code to allow Pley")
            }
            Text(code.userCode)
                .font(.system(size: 44, weight: .bold, design: .monospaced))
                .textSelection(.enabled)
                .accessibilityIdentifier("signIn.userCode")
            Text("Go to \(code.verificationURL.absoluteString) and type this code, or scan the QR code.")
                .multilineTextAlignment(.center)
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("Waiting for you to allow it…")
            }
            .foregroundStyle(.secondary)
            Text("The code runs out in \(SignInModel.minutes(secondsLeft)).")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .monospacedDigit()
            Button("Cancel", role: .cancel, action: cancel)
                .buttonStyle(.bordered)
                .accessibilityIdentifier("signIn.cancel")
        }
    }
}
