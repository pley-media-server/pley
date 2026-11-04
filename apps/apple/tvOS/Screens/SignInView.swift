import PleyCommon
import PleyNetwork
import PleyScreens
import SwiftUI

/// Signing in with a code: the QR code on the left; the code, where to
/// type it and Cancel on the right. Signing in again to the server in use
/// (for requests), Cancel and Menu go back to the app as it was.
struct SignInView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: SignInModel
    @FocusState private var focus: Control?

    private enum Control: Hashable {
        case cancel
        case again
    }

    init(app: AppModel, entry: ServerEntry) {
        self.app = app
        _model = StateObject(wrappedValue: SignInModel(app: app, entry: entry))
    }

    var body: some View {
        HStack(alignment: .center, spacing: 80) {
            qr
                .frame(width: 360, height: 360)
            VStack(alignment: .leading, spacing: 28) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Sign in to \(model.entry.name)").font(.title2.bold())
                    Text(Servers.displayAddress(model.entry.origin)).foregroundStyle(.secondary)
                }
                if let notice = model.notice {
                    Label(asSentence(notice), systemImage: "exclamationmark.triangle").foregroundStyle(.orange)
                }
                if model.unencryptedWarning {
                    Label(asSentence(PleyError.unencryptedMessage), systemImage: "lock.open")
                        .foregroundStyle(.orange)
                }
                details
                HStack(spacing: 40) {
                    if case .failed = model.phase {
                        Button("Get a new code") { model.signInWithCode() }
                            .focused($focus, equals: .again)
                    }
                    Button("Cancel", role: .cancel) { cancel() }
                    .focused($focus, equals: .cancel)
                    .accessibilityIdentifier("signIn.cancel")
                }
            }
            .frame(maxWidth: 900, alignment: .leading)
        }
        .padding(80)
        .defaultFocus($focus, .cancel)
        .onExitCommand(perform: again ? { cancel() } : nil)
        .onAppear { model.signInWithCode() }
        .onDisappear { model.cancel() }
    }

    /// Signing in again to the server in use, with the app behind it.
    private var again: Bool { app.user != nil && app.server?.origin == model.entry.origin }

    private func cancel() {
        model.cancel()
        if again {
            app.cancelSignInAgain()
        } else {
            app.closeSignIn()
        }
    }

    @ViewBuilder private var qr: some View {
        if case .code(_, let image?) = model.phase {
            Image(decorative: image, scale: 1)
                .interpolation(.none)
                .resizable()
                .accessibilityLabel("QR code to allow Pley")
        } else {
            RoundedRectangle(cornerRadius: 12).fill(.quaternary)
        }
    }

    @ViewBuilder private var details: some View {
        switch model.phase {
        case .code(let code, _):
            Text(code.userCode)
                .font(.system(size: 88, weight: .bold, design: .monospaced))
                .accessibilityIdentifier("signIn.userCode")
            Text("Go to \(code.verificationURL.absoluteString) on your phone or computer and type this code, "
                + "or scan the QR code.")
            HStack(spacing: 16) {
                ProgressView()
                Text("Waiting for you to allow it…")
            }
            .foregroundStyle(.secondary)
            Text("The code runs out in \(SignInModel.minutes(model.secondsLeft)).")
                .font(.callout)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        case .failed(let why):
            Text(asSentence(why)).foregroundStyle(.red)
        case .idle, .browser, .working:
            ProgressView()
        }
    }
}
