import PleyModel
import PleyScreens
import SwiftUI

/// Who's watching: the household's profiles in a row across the middle of
/// the screen, a profile's PIN, and Sign out below them, for a profile
/// that needs its password or an account that isn't the one wanted.
struct ProfilesView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ProfilesModel
    @State private var confirmSignOut = false
    /// The row's width, so the profiles sit in its middle while they fit.
    @State private var rowWidth: CGFloat = 0
    @FocusState private var focused: HouseholdProfile.ID?

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: ProfilesModel(app: app))
    }

    var body: some View {
        VStack(spacing: 60) {
            Text("Who's watching?").font(.largeTitle.bold())
            if model.loading { ProgressView() }
            ScrollView(.horizontal) {
                // Tops lined up: a profile with a line under its name stays level with the others.
                HStack(alignment: .top, spacing: 60) {
                    ForEach(model.profiles) { profile in
                        card(profile)
                    }
                }
                .padding(60)
                .frame(minWidth: rowWidth)
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { rowWidth = $0 }
            .focusSection()
            // The focus goes to the first profile that can be chosen once they come,
            // not left on Sign out, the only button before them.
            .task(id: model.profiles.map(\.id)) {
                if let first = model.profiles.first(where: ProfilesModel.canChoose) { focused = first.id }
            }
            if let error = model.error {
                Text(asSentence(error)).foregroundStyle(.red).multilineTextAlignment(.center)
            }
            Button("Sign out", role: .destructive) { confirmSignOut = true }
                .accessibilityIdentifier("profiles.signOut")
                .focusSection()
        }
        .padding(.vertical, 60)
        .task { await model.load() }
        .sheet(item: $model.pinFor) { profile in
            PinView(model: model, profile: profile)
        }
        .alert("Sign out of \(app.server?.name ?? localized("this server"))?", isPresented: $confirmSignOut) {
            Button("Sign out", role: .destructive) { Task { await app.signOut() } }
            Button("Cancel", role: .cancel) {}
        }
    }

    private func card(_ profile: HouseholdProfile) -> some View {
        VStack(spacing: 20) {
            Button {
                Task { await model.choose(profile) }
            } label: {
                ZStack {
                    AvatarView(app: app, name: profile.name, path: profile.avatarUrl, color: profile.avatarColor,
                               size: 220)
                    if model.switching == profile.id {
                        Color.black.opacity(0.4)
                        ProgressView()
                    }
                }
                .frame(width: 220, height: 220)
            }
            .buttonStyle(AvatarButtonStyle())
            .focused($focused, equals: profile.id)
            .disabled(!ProfilesModel.canChoose(profile) || model.switching != nil)
            .accessibilityLabel(profile.name)
            .accessibilityIdentifier("profiles.profile.\(profile.name)")
            HStack(spacing: 8) {
                Text(profile.name).font(.headline).lineLimit(1)
                if profile.needs == .pin { Image(systemName: "lock.fill").accessibilityLabel("PIN") }
            }
            if profile.current {
                Text("In use").font(.callout).foregroundStyle(.secondary)
            } else if profile.needs == .password {
                Text("Needs its password").font(.callout).foregroundStyle(.secondary)
            } else if profile.needs == .locked {
                Text("Locked").font(.callout).foregroundStyle(.secondary)
            }
        }
        .frame(width: 260)
        .opacity(ProfilesModel.canChoose(profile) ? 1 : 0.5)
    }
}

/// Asks for a profile's PIN with the number pad.
private struct PinView: View {
    @ObservedObject var model: ProfilesModel
    let profile: HouseholdProfile

    var body: some View {
        VStack(spacing: 40) {
            Text("\(profile.name)'s PIN").font(.title2.bold())
            SecureField("PIN", text: $model.pin)
                .keyboardType(.numberPad)
                .frame(maxWidth: 600)
                .onSubmit { Task { await model.submitPin() } }
                .accessibilityIdentifier("profiles.pin")
            if let error = model.pinError {
                Text(asSentence(error)).foregroundStyle(.red)
            }
            HStack(spacing: 40) {
                Button("Continue") { Task { await model.submitPin() } }
                    .disabled(model.switching != nil)
                    .accessibilityIdentifier("profiles.pin.continue")
                Button("Cancel", role: .cancel) { model.cancelPin() }
                if model.switching != nil { ProgressView() }
            }
        }
        .padding(80)
    }
}

/// A profile's picture as a button: round, with no card behind it. With
/// the focus it grows and lifts on a shadow, with a white ring under
/// Reduce Motion instead; pressed, it sinks a little.
private struct AvatarButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Avatar(configuration: configuration)
    }

    private struct Avatar: View {
        let configuration: ButtonStyleConfiguration
        @Environment(\.isFocused) private var focused
        @Environment(\.accessibilityReduceMotion) private var reduceMotion

        var body: some View {
            configuration.label
                .clipShape(Circle())
                .overlay {
                    if focused && reduceMotion {
                        Circle().strokeBorder(.white, lineWidth: 6)
                    }
                }
                .scaleEffect(reduceMotion ? 1 : configuration.isPressed ? 1.05 : focused ? 1.15 : 1)
                .shadow(color: .black.opacity(focused ? 0.5 : 0), radius: 20, y: 12)
                .animation(Motion.animation(.easeOut(duration: 0.15), reduced: reduceMotion), value: focused)
                .animation(Motion.animation(.easeOut(duration: 0.1), reduced: reduceMotion),
                           value: configuration.isPressed)
        }
    }
}
