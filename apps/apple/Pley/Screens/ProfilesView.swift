import PleyAuth
import PleyModel
import PleyScreens
import SwiftUI

/// Who's watching: the household's profiles, and a profile's PIN.
struct ProfilesView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ProfilesModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: ProfilesModel(app: app))
    }

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                if model.loading {
                    ProgressView().controlSize(.large).padding()
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 24)], spacing: 28) {
                    ForEach(model.profiles) { profile in
                        Button {
                            Task { await model.choose(profile) }
                        } label: {
                            card(profile)
                        }
                        .buttonStyle(.plain)
                        .disabled(!ProfilesModel.canChoose(profile) || model.switching != nil)
                        .accessibilityLabel(profile.name)
                        .accessibilityIdentifier("profiles.profile.\(profile.name)")
                    }
                }
                if let error = model.error {
                    Text(asSentence(error))
                        .foregroundStyle(.red)
                        .multilineTextAlignment(.center)
                }
            }
            .padding(24)
        }
        .navigationTitle("Who's watching?")
        .task { await model.load() }
        .sheet(item: $model.pinFor) { profile in
            PinSheet(model: model, profile: profile)
        }
    }

    private func card(_ profile: HouseholdProfile) -> some View {
        VStack(spacing: 10) {
            ZStack {
                AvatarView(app: app, name: profile.name, path: profile.avatarUrl, color: profile.avatarColor, size: 96)
                if model.switching == profile.id {
                    Circle().fill(.black.opacity(0.4)).frame(width: 96, height: 96)
                    ProgressView().tint(.white)
                }
            }
            .overlay {
                if profile.current {
                    Circle().stroke(Color.accentColor, lineWidth: 3).frame(width: 104, height: 104)
                }
            }
            HStack(spacing: 4) {
                Text(profile.name).lineLimit(1)
                if profile.needs == .pin {
                    Image(systemName: "lock.fill").font(.caption).accessibilityLabel("PIN")
                }
            }
            if profile.needs == .password {
                Text("Needs its password").font(.caption).foregroundStyle(.secondary)
            } else if profile.needs == .locked {
                Text("Locked").font(.caption).foregroundStyle(.secondary)
            }
        }
        .opacity(ProfilesModel.canChoose(profile) ? 1 : 0.5)
        .contentShape(Rectangle())
    }
}

/// Asks for a profile's PIN.
private struct PinSheet: View {
    @ObservedObject var model: ProfilesModel
    let profile: HouseholdProfile

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    SecureField("PIN", text: $model.pin)
                        #if os(iOS)
                        .keyboardType(.numberPad)
                        #endif
                        .textContentType(.password)
                        .onSubmit { Task { await model.submitPin() } }
                        .accessibilityIdentifier("profiles.pin")
                } header: {
                    Text("\(profile.name)'s PIN")
                } footer: {
                    if let error = model.pinError {
                        Text(asSentence(error)).foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle(profile.name)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { model.cancelPin() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if model.switching != nil {
                        ProgressView()
                    } else {
                        Button("Continue") { Task { await model.submitPin() } }
                            .accessibilityIdentifier("profiles.pin.continue")
                    }
                }
            }
        }
        #if os(macOS)
        .frame(minWidth: 360, minHeight: 200)
        #endif
        .presentationDetents([.medium])
    }
}
