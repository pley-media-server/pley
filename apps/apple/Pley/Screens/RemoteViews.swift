import PleyModel
import PleyScreens
import SwiftUI

// The remote (docs/APPLE.md, Remote control): what the profile plays on
// its other devices, and the commands each player obeys. On iPhone and
// iPad a bar above the tabs opens it as a sheet; on the Mac a toolbar
// button opens it as a popover. Both show `RemotePanel`.

#if os(iOS)
/// What plays on the profile's other devices, above the tabs: the device
/// and what plays there, and Play or Pause when there is one device whose
/// player obeys it. A tap elsewhere opens the remote. From iOS 26.1 it is
/// the tab bar's bottom accessory, which draws its own glass; before, and
/// at the foot of the sidebar layout's page, a card of its own.
struct RemoteBar: View {
    @ObservedObject var remote: RemoteModel
    /// In the tab bar's accessory, which draws its own background and has a height of its own.
    var inAccessory = false
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        let others = remote.others
        if !others.isEmpty {
            let names = remote.names
            let device = others.count == 1 ? names[others[0].id] ?? "" : Words.pluralize(others.count, "device")
            let playing = others.count == 1 ? Words.remoteTitle(others[0].session)
                : others.compactMap { names[$0.id] }.joined(separator: ", ")
            let lines = typeSize.isAccessibilitySize && !inAccessory ? 3 : 1
            HStack(spacing: 4) {
                Button(action: open) {
                    HStack(spacing: 12) {
                        Image(systemName: "av.remote")
                            .font(inAccessory ? .body : .title3)
                            .foregroundStyle(Color.accentColor)
                            .frame(minWidth: 32)
                        VStack(alignment: .leading, spacing: inAccessory ? 0 : 2) {
                            Text(device)
                                .font(inAccessory ? .caption : .footnote)
                                .foregroundStyle(.secondary)
                                .lineLimit(lines)
                            Text(playing)
                                .font(.subheadline.weight(.semibold))
                                .lineLimit(lines)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(minHeight: 44)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(Words.remoteLabel(others.count)), \(device), \(playing)")
                .accessibilityHint("Opens the remote")
                .accessibilityAddTraits(.isButton)
                .accessibilityIdentifier("remote.bar")
                if others.count == 1 {
                    toggle(others[0], device: device)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, inAccessory ? 0 : 6)
            // Larger, its words and symbols would spill out of the accessory.
            .dynamicTypeSize(...(inAccessory ? DynamicTypeSize.xxxLarge : .accessibility5))
            .background {
                if !inAccessory {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(.regularMaterial)
                        .shadow(color: .black.opacity(0.12), radius: 6, y: 2)
                }
            }
            .padding(.horizontal, inAccessory ? 0 : 8)
            .padding(.vertical, inAccessory ? 0 : 6)
        }
    }

    /// Play or Pause for the one device playing, when its player obeys it:
    /// pausing the TV is what the remote is mostly for.
    @ViewBuilder private func toggle(_ entry: RemoteModel.Entry, device: String) -> some View {
        let paused = entry.session.state == .paused
        if let command: PlayerCommand = paused ? (entry.session.obeys(.play) ? .play : nil)
            : (entry.session.obeys(.pause) ? .pause : nil) {
            let play = paused
            Button {
                Task { await remote.send(command, to: entry) }
            } label: {
                Image(systemName: play ? "play.fill" : "pause.fill")
                    .font(.title3)
                    .frame(minWidth: 44, minHeight: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            .opacity(remote.waiting.contains(entry.id) ? 0.5 : 1)
            .accessibilityLabel(play ? Words.remotePlay(device) : Words.remotePause(device))
            .accessibilityIdentifier("remote.bar.toggle")
        }
    }
}

/// The remote's button beside the mini player in the tab bar's accessory,
/// while this device plays music and another device plays too: the
/// accessory holds one of them, and the music playing here comes first.
struct RemoteAccessoryButton: View {
    @ObservedObject var remote: RemoteModel
    let open: () -> Void

    var body: some View {
        let count = remote.others.count
        if count > 0 {
            Button(action: open) {
                Image(systemName: "av.remote")
                    .font(.body)
                    .foregroundStyle(Color.accentColor)
                    .frame(minWidth: 44, minHeight: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.trailing, 8)
            .accessibilityLabel(Words.remoteLabel(count))
            .accessibilityHint("Opens the remote")
            .accessibilityIdentifier("remote.bar")
        }
    }
}

/// The remote as a sheet over the app, closed with Done, or by itself once
/// nothing plays on another device.
struct RemoteSheet: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    let close: () -> Void

    var body: some View {
        NavigationStack {
            ScrollView {
                RemotePanel(app: app, remote: remote)
                    .frame(maxWidth: 640)
                    .frame(maxWidth: .infinity)
            }
            .navigationTitle(Words.remoteTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: close).accessibilityIdentifier("remote.done")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .accessibilityIdentifier("remote.sheet")
    }
}
#endif

#if os(macOS)
/// The toolbar's remote button, while another of the profile's devices
/// plays: it opens the remote as a popover, as the web's header does.
struct RemoteToolbarButton: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    @State private var open = false

    var body: some View {
        let count = remote.others.count
        if count > 0 {
            Button {
                open.toggle()
            } label: {
                Label(Words.remoteTitle, systemImage: "av.remote")
            }
            .help(Words.remoteLabel(count))
            .accessibilityLabel(Words.remoteLabel(count))
            .accessibilityIdentifier("remote.button")
            .popover(isPresented: Binding(get: { open && count > 0 }, set: { open = $0 }), arrowEdge: .bottom) {
                ScrollView {
                    RemotePanel(app: app, remote: remote)
                }
                .frame(width: 380)
                .frame(maxHeight: 560)
                .accessibilityLabel(Words.remoteTitle)
                .accessibilityIdentifier("remote.panel")
            }
        }
    }
}
#endif

/// The profile's other devices, one after another: what plays on each,
/// how far, and the controls its player listed (Stop always).
struct RemotePanel: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel

    var body: some View {
        let names = remote.names
        VStack(spacing: 0) {
            ForEach(Array(remote.others.enumerated()), id: \.element.id) { index, entry in
                if index > 0 { Divider() }
                RemoteRow(app: app, remote: remote, entry: entry,
                          device: names[entry.id] ?? Words.unnamedDevice)
            }
        }
    }
}

/// One device in the remote: the poster (or the channel's logo), the title
/// (or the channel and what is on), the device's name, how far it is (a
/// seek slider when its player seeks), and its controls.
private struct RemoteRow: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    let entry: RemoteModel.Entry
    let device: String
    /// The slider's place while it is dragged.
    @State private var seeking: Double?
    @Environment(\.dynamicTypeSize) private var typeSize

    private var session: PlayingSession { entry.session }
    private var waiting: Bool { remote.waiting.contains(entry.id) }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            heading
            if !entry.live, let duration = session.durationMs, duration > 0 {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    progress(RemoteModel.position(entry, at: context.date), duration)
                }
            }
            controls
            if let error = remote.errors[entry.id] {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("remote.error.\(entry.id)")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(device)
        .accessibilityValue(waiting ? Words.remoteWaiting(device) : "")
        .accessibilityIdentifier("remote.session.\(entry.id)")
        .onValueChange(of: remote.errors[entry.id]) { error in
            if let error { announce(error) }
        }
    }

    // MARK: What plays

    private var heading: some View {
        HStack(spacing: 12) {
            art
            VStack(alignment: .leading, spacing: 2) {
                Text(device)
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                Text(Words.remoteTitle(session))
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(typeSize.isAccessibilitySize ? 4 : 2)
                if entry.live, let now = session.channel?.now, !now.isEmpty {
                    Text(now)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private var art: some View {
        if entry.live, let channel = session.channel {
            ZStack {
                RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.12))
                Text(String(channel.name.prefix(1))).font(.headline).foregroundStyle(.secondary)
                if channel.logo != nil {
                    ServerImage(app: app, path: channel.logo, width: 56, placeholder: false)
                        .scaledToFit()
                        .padding(3)
                }
            }
            .frame(width: 56, height: 40)
            .accessibilityHidden(true)
        } else if let item = session.item {
            let square = item.kind == .track
            ServerImage(app: app, paths: RemoteModel.artPaths(session), width: 40,
                        symbol: square ? "music.note" : "film")
                .frame(width: 40, height: square ? 40 : 60)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                .accessibilityHidden(true)
        }
    }

    // MARK: How far

    private func progress(_ position: Int, _ duration: Int) -> some View {
        let shown = seeking.map { Int($0) } ?? position
        return HStack(spacing: 8) {
            Text(Words.clock(ms: shown))
                .accessibilityHidden(true)
            if session.obeys(.seek) {
                Slider(value: Binding(get: { seeking ?? Double(position) }, set: { seeking = $0 }),
                       in: 0...Double(duration), step: 1_000) { editing in
                    guard !editing, let to = seeking else { return }
                    seeking = nil
                    seek(Int(to))
                }
                .accessibilityLabel(Words.remoteSeek(device))
                .accessibilityValue(spoken(shown, duration))
                // VoiceOver's swipes go back and forward as the skip buttons do, one command each.
                .accessibilityAdjustableAction { direction in
                    switch direction {
                    case .increment: seek(min(position + Words.remoteSkipForwardMs, duration))
                    case .decrement: seek(max(position - Words.remoteSkipBackMs, 0))
                    @unknown default: break
                    }
                }
                .accessibilityIdentifier("remote.seek.\(entry.id)")
            } else {
                ProgressView(value: Double(min(position, duration)), total: Double(duration))
                    .accessibilityLabel(Words.remoteProgress(device))
                    .accessibilityValue(spoken(position, duration))
            }
            Text(Words.clock(ms: duration))
                .accessibilityHidden(true)
        }
        .font(.caption.monospacedDigit())
        .foregroundStyle(.secondary)
    }

    private func spoken(_ position: Int, _ duration: Int) -> String {
        "\(Words.clock(ms: position)) of \(Words.clock(ms: duration))"
    }

    // MARK: Controls

    @ViewBuilder private var controls: some View {
        if typeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: 8) {
                buttons
                stop
            }
        } else {
            HStack(spacing: 4) {
                buttons
                Spacer(minLength: 8)
                stop
            }
        }
    }

    private var buttons: some View {
        let paused = session.state == .paused
        let toggle: Control? = paused ? (session.obeys(.play) ? .play : nil) : (session.obeys(.pause) ? .pause : nil)
        return HStack(spacing: 4) {
            if session.obeys(.previous) {
                icon("backward.end.fill", Words.remotePrevious(device, live: entry.live), "previous") {
                    send(.previous)
                }
            }
            if session.obeys(.skip) {
                icon("gobackward.10", Words.remoteBack(device), "back") {
                    send(.skip(byMs: -Words.remoteSkipBackMs))
                }
            }
            if let toggle {
                Button {
                    send(toggle == .play ? .play : .pause)
                } label: {
                    Image(systemName: toggle == .play ? "play.fill" : "pause.fill")
                        .font(.title3)
                        .foregroundStyle(.white)
                        .frame(width: 44, height: 44)
                        .background(Circle().fill(Color.accentColor))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(toggle == .play ? Words.remotePlay(device) : Words.remotePause(device))
                .accessibilityIdentifier("remote.toggle.\(entry.id)")
            }
            if session.obeys(.skip) {
                icon("goforward.30", Words.remoteForward(device), "forward") {
                    send(.skip(byMs: Words.remoteSkipForwardMs))
                }
            }
            if session.obeys(.next) {
                icon("forward.end.fill", Words.remoteNext(device, live: entry.live), "next") { send(.next) }
            }
        }
        // Larger, five buttons spill out of an iPhone's width; VoiceOver and Large Content read their names.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .opacity(waiting ? 0.5 : 1)
    }

    private func icon(_ symbol: String, _ label: String, _ id: String,
                      action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.title3)
                .frame(minWidth: 44, minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
        .help(label)
        .accessibilityLabel(label)
        .accessibilityShowsLargeContentViewer {
            Label(label, systemImage: symbol)
        }
        .accessibilityIdentifier("remote.\(id).\(entry.id)")
    }

    private var stop: some View {
        Button(role: .destructive) {
            Task { await remote.stop(entry) }
        } label: {
            Label("Stop", systemImage: "stop.circle")
                .font(.subheadline.weight(.medium))
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .disabled(remote.stopping.contains(entry.id))
        .accessibilityLabel(Words.remoteStop(device))
        .accessibilityIdentifier("remote.stop.\(entry.id)")
    }

    private func send(_ command: PlayerCommand) {
        Task { await remote.send(command, to: entry) }
    }

    private func seek(_ positionMs: Int) {
        send(.seek(toMs: positionMs))
    }
}
