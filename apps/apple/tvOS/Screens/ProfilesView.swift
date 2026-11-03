import PleyModel
import PleyScreens
import SwiftUI

/// Who's watching: a row of the household's profiles, and a profile's PIN.
struct ProfilesView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ProfilesModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: ProfilesModel(app: app))
    }

    var body: some View {
        VStack(spacing: 60) {
            Text("Who's watching?").font(.largeTitle.bold())
            if model.loading { ProgressView() }
            ScrollView(.horizontal) {
                HStack(spacing: 60) {
                    ForEach(model.profiles) { profile in
                        card(profile)
                    }
                }
                .padding(60)
            }
            .focusSection()
            if let error = model.error {
                Text(asSentence(error)).foregroundStyle(.red).multilineTextAlignment(.center)
            }
        }
        .padding(.vertical, 60)
        .task { await model.load() }
        .sheet(item: $model.pinFor) { profile in
            PinView(model: model, profile: profile)
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
            .buttonStyle(.card)
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
