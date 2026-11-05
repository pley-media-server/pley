import PleyModel
import PleyScreens
import SwiftUI

// The remote (docs/APPLE.md, Remote control): the profile's other devices
// that are on, what plays on each, and the commands each player obeys. On
// iPhone and iPad a bar above the tabs opens it as a sheet; on the Mac a
// toolbar button opens it as a popover. Its list of devices is
// `RemoteDeviceList`, a device's controls `RemoteDeviceControls`.
// "Play on…" (`PlayOnMenu`) sends a page's title, music, playlist or
// channel to one of them.

extension DeviceKind {
    /// The symbol the remote shows for a device of this kind.
    var symbol: String {
        switch self {
        case .tv: "tv"
        case .phone: "iphone"
        case .tablet: "ipad"
        case .computer: "laptopcomputer"
        case .browser: "globe"
        default: "questionmark"
        }
    }
}

extension RemoteModel.Device {
    /// What it plays, as its row says it; "Ready to play" while it plays nothing.
    var line: String { entry.map { Words.remotePlaying($0.session) } ?? Words.readyToPlay }
}

#if os(iOS)
/// The profile's other devices, above the tabs: the device and what plays
/// there (or "Ready to play"), or how many devices and their names, and
/// Play or Pause when there is one device whose player obeys it. A tap
/// elsewhere opens the remote. From iOS 26.1 it is the tab bar's bottom
/// accessory, which draws its own glass; before, and at the foot of the
/// sidebar layout's page, a card of its own.
struct RemoteBar: View {
    @ObservedObject var remote: RemoteModel
    /// In the tab bar's accessory, which draws its own background and has a height of its own.
    var inAccessory = false
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        let devices = remote.devices
        if !devices.isEmpty {
            let names = remote.names
            let one = devices.count == 1 ? devices[0] : nil
            let device = one.map { names[$0.id] ?? Words.unnamedDevice } ?? Words.pluralize(devices.count, "device")
            let playing = one?.line ?? devices.compactMap { names[$0.id] }.joined(separator: ", ")
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
                .accessibilityLabel("\(Words.remoteLabel(devices.count)), \(device), \(playing)")
                .accessibilityHint("Opens the remote")
                .accessibilityAddTraits(.isButton)
                .accessibilityIdentifier("remote.bar")
                if let entry = one?.entry {
                    toggle(entry, device: device)
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
            // Clear of the page's edges, and of an iPad window's rounded corners below.
            .padding(.horizontal, inAccessory ? 0 : 16)
            .padding(.top, inAccessory ? 0 : 8)
            .padding(.bottom, inAccessory ? 0 : 16)
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
/// while this device plays music and another device is on: the accessory
/// holds one of them, and the music playing here comes first.
struct RemoteAccessoryButton: View {
    @ObservedObject var remote: RemoteModel
    let open: () -> Void

    var body: some View {
        let count = remote.devices.count
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
/// no other device is on. On a phone the devices are a list, and choosing
/// one opens its controls on a page of their own (Back returns to the
/// list); one device alone shows its controls at once. On an iPad the
/// controls of the device chosen show below the list.
struct RemoteSheet: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    /// The app's wide layout (an iPad at regular width): told by the shell,
    /// since a sheet on an iPad is compact itself.
    let wide: Bool
    let close: () -> Void

    var body: some View {
        NavigationStack(path: path) {
            root
                .navigationTitle(Words.remoteTitle)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { done }
                .navigationDestination(for: String.self) { id in
                    page(id)
                }
        }
        // "Playing on …" after a cast, over the sheet that opens with it.
        .overlay(alignment: .bottom) { ToastView(shell: app.shell) }
        .presentationDetents([.medium, .large])
        .accessibilityIdentifier("remote.sheet")
    }

    /// Whether a device's controls are a page of their own: on a phone, with several devices.
    private var paged: Bool { !wide && remote.devices.count > 1 }

    /// The page showing over the list: the device chosen, while it is on.
    private var path: Binding<[String]> {
        Binding(get: {
            guard paged, let chosen = remote.chosen, remote.devices.contains(where: { $0.id == chosen }) else {
                return []
            }
            return [chosen]
        }, set: { remote.chosen = $0.last })
    }

    @ViewBuilder private var root: some View {
        let devices = remote.devices
        if wide {
            ScrollView {
                RemotePanel(app: app, remote: remote)
                    .frame(maxWidth: 640)
                    .frame(maxWidth: .infinity)
            }
        } else if devices.count == 1 {
            ScrollView {
                RemoteDeviceControls(app: app, remote: remote, device: devices[0])
            }
        } else {
            ScrollView {
                RemoteDeviceList(remote: remote, selected: nil, chevrons: true)
            }
        }
    }

    @ViewBuilder private func page(_ id: String) -> some View {
        if let device = remote.devices.first(where: { $0.id == id }) {
            ScrollView {
                RemoteDeviceControls(app: app, remote: remote, device: device)
            }
            .navigationTitle(remote.name(device))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { done }
        }
    }

    private var done: some ToolbarContent {
        ToolbarItem(placement: .confirmationAction) {
            Button("Done", action: close).accessibilityIdentifier("remote.done")
        }
    }
}
#endif

#if os(macOS)
/// The toolbar's remote button, while another of the profile's devices is
/// on: it opens the remote as a popover, as the web's header does.
struct RemoteToolbarButton: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel

    var body: some View {
        let count = remote.devices.count
        if count > 0 {
            Button {
                remote.presented.toggle()
            } label: {
                Label(Words.remoteTitle, systemImage: "av.remote")
            }
            .help(Words.remoteLabel(count))
            .accessibilityLabel(Words.remoteLabel(count))
            .accessibilityIdentifier("remote.button")
            // Gone once no other device is on: it opens closed when one is again.
            .onDisappear { remote.presented = false }
            .popover(isPresented: Binding(get: { remote.presented && count > 0 }, set: { remote.presented = $0 }),
                     arrowEdge: .bottom) {
                ScrollView {
                    RemotePanel(app: app, remote: remote, heading: true)
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

/// The profile's other devices, and below them the controls of the one
/// chosen (or of the only one): the Mac's popover, and the iPad's sheet.
struct RemotePanel: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    /// "Your devices" above the list, where no navigation title says it.
    var heading = false

    var body: some View {
        let selected = remote.selected
        VStack(alignment: .leading, spacing: 0) {
            if heading {
                Text(Words.remoteTitle)
                    .font(.headline)
                    .padding(.horizontal, 16)
                    .padding(.top, 14)
                    .padding(.bottom, 4)
                    .accessibilityAddTraits(.isHeader)
            }
            RemoteDeviceList(remote: remote, selected: selected?.id, chevrons: false)
            if let selected {
                Divider()
                RemoteDeviceControls(app: app, remote: remote, device: selected)
            }
        }
    }
}

/// The devices, those playing first, then those ready to play, each by
/// name: its kind's symbol, its name, what it plays with how far, or
/// "Ready to play". Choosing one shows its controls.
private struct RemoteDeviceList: View {
    @ObservedObject var remote: RemoteModel
    let selected: String?
    /// Each row opens a page of its own (a phone's).
    let chevrons: Bool

    var body: some View {
        let names = remote.names
        VStack(spacing: 0) {
            ForEach(Array(remote.devices.enumerated()), id: \.element.id) { index, device in
                if index > 0 { Divider().padding(.leading, 60) }
                RemoteDeviceRow(device: device, name: names[device.id] ?? Words.unnamedDevice,
                                selected: device.id == selected, chevron: chevrons) {
                    remote.chosen = device.id
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("remote.devices")
    }
}

/// One device in the list.
private struct RemoteDeviceRow: View {
    let device: RemoteModel.Device
    let name: String
    let selected: Bool
    let chevron: Bool
    let choose: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        Button(action: choose) {
            HStack(spacing: 12) {
                Image(systemName: device.kind.symbol)
                    .font(.title3)
                    .foregroundStyle(device.entry == nil ? Color.secondary : Color.accentColor)
                    .frame(width: 32)
                VStack(alignment: .leading, spacing: 3) {
                    Text(name)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                    Text(device.line)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(typeSize.isAccessibilitySize ? 4 : 1)
                    if let entry = device.entry, !entry.live, let duration = entry.session.durationMs, duration > 0 {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            ProgressView(value: Double(min(RemoteModel.position(entry, at: context.date), duration)),
                                         total: Double(duration))
                        }
                        .accessibilityHidden(true)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if chevron {
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(selected ? Color.accentColor.opacity(0.12) : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(name), \(device.line)")
        .accessibilityHint("Shows its controls")
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("remote.device.\(device.id)")
    }
}

/// A device's controls: what plays there and what its player obeys, or,
/// on one ready to play, how to send it something.
struct RemoteDeviceControls: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    let device: RemoteModel.Device

    var body: some View {
        let name = remote.name(device)
        if let entry = device.entry {
            RemotePlayingControls(app: app, remote: remote, entry: entry, device: name)
        } else {
            VStack(spacing: 8) {
                Image(systemName: device.kind.symbol)
                    .font(.largeTitle)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(name)
                    .font(.headline)
                    .multilineTextAlignment(.center)
                Text(Words.nothingPlaying)
                    .foregroundStyle(.secondary)
                Text(Words.castHint)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 24)
            .padding(.vertical, 20)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("remote.idle.\(device.id)")
        }
    }
}

/// A playing device's controls, top to bottom: the poster (or the
/// channel's logo) with what plays; how far it is (a seek slider when its
/// player seeks; on Live TV, how far along the programme on is by the
/// clock, its times and what's next); Previous, Back 10 seconds, Play or
/// Pause, Forward 30 seconds and Next, each where its player obeys it (Next
/// shows, dimmed, for whatever has one), or a channel's rocker; then
/// Audio, Subtitles and Quality as symbols, each where it has something to
/// pick, and Stop, which every player obeys, once asked.
private struct RemotePlayingControls: View {
    let app: AppModel
    @ObservedObject var remote: RemoteModel
    let entry: RemoteModel.Entry
    let device: String
    /// The slider's place while it is dragged.
    @State private var seeking: Double?
    /// Stop's confirmation shows.
    @State private var confirmingStop = false
    @Environment(\.dynamicTypeSize) private var typeSize

    private var session: PlayingSession { entry.session }
    private var waiting: Bool { remote.waiting.contains(entry.id) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            heading
            if entry.live {
                programme
            } else if let duration = session.durationMs, duration > 0 {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    progress(RemoteModel.position(entry, at: context.date), duration)
                }
            }
            transport
            pickersAndStop
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
        let shown = Words.remoteHeading(session)
        return HStack(spacing: 12) {
            art
            VStack(alignment: .leading, spacing: 2) {
                Text(device)
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(.secondary)
                    .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                Text(shown.title)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(typeSize.isAccessibilitySize ? 4 : 2)
                if let subtitle = shown.subtitle {
                    Text(subtitle)
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

    /// On Live TV, no seek: how far along the programme on is, by this
    /// device's clock between its start and end ("20:00 – 21:00" in the
    /// system's time format), then what's next, where the guide says.
    @ViewBuilder private var programme: some View {
        // Nothing at all while the guide says nothing of the channel.
        if let channel = session.channel, channel.progress(at: 0) != nil || !(channel.next ?? "").isEmpty {
            let next = channel.next.flatMap { $0.isEmpty ? nil : $0 }
            VStack(alignment: .leading, spacing: 4) {
                if let start = channel.nowStart, let stop = channel.nowStop, stop > start {
                    let times = "\(LiveTvRules.time(start)) – \(LiveTvRules.time(stop))"
                    TimelineView(.periodic(from: .now, by: 15)) { context in
                        ProgressView(value: channel.progress(at: Int64(context.date.timeIntervalSince1970)) ?? 0)
                            .accessibilityLabel(Words.remoteProgress(device))
                            .accessibilityValue([channel.now, times].compactMap { $0 }.joined(separator: ", "))
                    }
                    Text(times)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                }
                if let next {
                    Text(Words.remoteNextProgramme(next))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                }
            }
            .accessibilityIdentifier("remote.programme.\(entry.id)")
        }
    }

    // MARK: Controls

    /// A title's buttons: Previous, Back 10 seconds, Play or Pause, Forward
    /// 30 seconds and Next. On Live TV, the channel rocker (down, "Channel",
    /// up), as a TV remote has it, with Play or Pause beside it where the
    /// player takes it.
    private var transport: some View {
        let paused = session.state == .paused
        let toggle: Control? = paused ? (session.obeys(.play) ? .play : nil) : (session.obeys(.pause) ? .pause : nil)
        // Whatever has a next shows Next: an episode, a track, a title with one after it.
        let kind = session.item?.kind
        let next = session.obeys(.next) || kind == .episode || kind == .track
        return HStack(spacing: entry.live ? 16 : 4) {
            Spacer(minLength: 0)
            if entry.live {
                channelRocker
                if let toggle { toggleButton(toggle) }
            } else {
                if session.obeys(.previous) {
                    icon("backward.end.fill", Words.remotePrevious(device, live: false), "previous") {
                        send(.previous)
                    }
                }
                if session.obeys(.skip) {
                    icon("gobackward.10", Words.remoteBack(device), "back") {
                        send(.skip(byMs: -Words.remoteSkipBackMs))
                    }
                }
                if let toggle { toggleButton(toggle) }
                if session.obeys(.skip) {
                    icon("goforward.30", Words.remoteForward(device), "forward") {
                        send(.skip(byMs: Words.remoteSkipForwardMs))
                    }
                }
                if next {
                    icon("forward.end.fill", Words.remoteNext(device, live: false), "next") { send(.next) }
                        .disabled(!session.obeys(.next))
                        .opacity(session.obeys(.next) ? 1 : 0.35)
                }
            }
            Spacer(minLength: 0)
        }
        // Larger, five buttons spill out of an iPhone's width; VoiceOver and Large Content read their names.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .opacity(waiting ? 0.5 : 1)
    }

    /// Play or Pause, the big round one.
    private func toggleButton(_ toggle: Control) -> some View {
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

    /// Channel down and up in one capsule, as a TV remote's rocker: each
    /// half dimmed when the player doesn't take it (`previous`, `next`).
    private var channelRocker: some View {
        HStack(spacing: 0) {
            icon("chevron.down", Words.remotePrevious(device, live: true), "previous") { send(.previous) }
                .disabled(!session.obeys(.previous))
                .opacity(session.obeys(.previous) ? 1 : 0.35)
            Text(Words.remoteChannelTitle)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .padding(.horizontal, 2)
                .accessibilityHidden(true)
            icon("chevron.up", Words.remoteNext(device, live: true), "next") { send(.next) }
                .disabled(!session.obeys(.next))
                .opacity(session.obeys(.next) ? 1 : 0.35)
        }
        .padding(.horizontal, 6)
        .background(Capsule().fill(Color.secondary.opacity(0.12)))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Words.remoteChannelTitle)
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

    /// Audio, Subtitles and Quality as symbols, each where its player obeys
    /// it and there is something to pick (more than one audio track; any
    /// subtitles; the qualities its Quality menu offers), then Stop at the
    /// end. Each is a menu of the choices with the one playing ticked, as
    /// the system draws menus, its button marked while it is open.
    @ViewBuilder private var pickersAndStop: some View {
        let audio = session.obeys(.audio) && session.tracks.audio.count > 1
        let subtitles = session.obeys(.subtitles) && !session.tracks.subtitles.isEmpty
        let quality = session.obeys(.quality) && !session.qualities.isEmpty
        let pickers = HStack(spacing: 8) {
            if audio { audioMenu }
            if subtitles { subtitlesMenu }
            if quality { qualityMenu }
        }
        if typeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: 8) {
                if audio || subtitles || quality { pickers }
                stop
            }
        } else {
            HStack(spacing: 8) {
                pickers
                Spacer(minLength: 8)
                stop
            }
        }
    }

    /// The audio tracks, labelled as the player's own menu labels them, the one playing ticked.
    private var audioMenu: some View {
        picker(Words.remoteAudioTitle, symbol: "speaker.wave.2", label: Words.remoteAudio(device), id: "audio",
               selection: Binding(get: { session.audioIndex ?? Int.min }, set: { index in
                   guard index != session.audioIndex else { return }
                   send(.audio(index))
               })) {
            ForEach(session.tracks.audio) { track in
                Text(Words.stream(track.stream(.audio))).tag(track.index)
            }
        }
    }

    /// Off, then the subtitle tracks, the ones showing ticked.
    private var subtitlesMenu: some View {
        picker(Words.remoteSubtitlesTitle, symbol: "captions.bubble", label: Words.remoteSubtitles(device),
               id: "subtitles", selection: Binding(get: { session.subtitleIndex ?? Int.min }, set: { index in
                   guard index != session.subtitleIndex else { return }
                   send(.subtitles(index == SessionTracks.off ? nil : index))
               })) {
            Text(Words.remoteSubtitlesOff).tag(SessionTracks.off)
            ForEach(session.tracks.subtitles) { track in
                Text(Words.stream(track.stream(.subtitle))).tag(track.index)
            }
        }
    }

    /// The qualities its Quality menu offers for the file, by their names
    /// there ("8 Mbps 1080p"), its pick ticked; none until its player says.
    private var qualityMenu: some View {
        picker(Words.remoteQualityTitle, symbol: "slider.horizontal.3", label: Words.remoteQuality(device),
               id: "quality", selection: Binding(get: { session.quality ?? "" }, set: { id in
                   guard !id.isEmpty, id != session.quality else { return }
                   send(.quality(id))
               })) {
            ForEach(session.qualities) { quality in
                Text(quality.label).tag(quality.id)
            }
        }
    }

    /// One of the pickers: a symbol whose menu holds `choices`, the one
    /// `selection` names ticked. VoiceOver reads `label` ("Audio on Living
    /// room TV"); the pointer's tooltip and Large Content say `title`.
    private func picker<Value: Hashable, Choices: View>(
        _ title: String, symbol: String, label: String, id: String, selection: Binding<Value>,
        @ViewBuilder choices: () -> Choices
    ) -> some View {
        let content = choices()
        return Menu {
            Picker(title, selection: selection) { content }
                .pickerStyle(.inline)
        } label: {
            Image(systemName: symbol)
                .font(.body.weight(.medium))
                .frame(minWidth: 28, minHeight: 28)
        }
        .menuStyle(.button)
        .buttonStyle(.bordered)
        .help(label)
        .opacity(waiting ? 0.5 : 1)
        .accessibilityLabel(label)
        .accessibilityShowsLargeContentViewer {
            Label(title, systemImage: symbol)
        }
        .accessibilityIdentifier("remote.\(id).\(entry.id)")
    }

    /// Stop asks first, in the system's confirmation: Stop, or Cancel.
    private var stop: some View {
        Button(role: .destructive) {
            confirmingStop = true
        } label: {
            Label(Words.remoteStopTitle, systemImage: "stop.circle")
                .font(.subheadline.weight(.medium))
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .disabled(remote.stopping.contains(entry.id))
        .accessibilityLabel(Words.remoteStop(device))
        .accessibilityIdentifier("remote.stop.\(entry.id)")
        // On the button, so an iPad's and a Mac's show beside it.
        .confirmationDialog(Words.remoteStopQuestion(device), isPresented: $confirmingStop,
                            titleVisibility: .visible) {
            Button(Words.remoteStopTitle, role: .destructive) {
                Task { await remote.stop(entry) }
            }
            .accessibilityIdentifier("remote.stop.confirm")
            Button(Words.remoteCancel, role: .cancel) {}
        }
    }

    private func send(_ command: PlayerCommand) {
        Task { await remote.send(command, to: entry) }
    }

    private func seek(_ positionMs: Int) {
        send(.seek(toMs: positionMs))
    }
}

/// "Play on…" beside a page's Play ("Watch on…" for a channel): the
/// profile's other devices that take what it plays, each with what it
/// plays now, as the remote lists them; choosing one sends it there. Beside
/// Shuffle, "Shuffle on…" in it sends it shuffled. Nothing shows while no
/// other device takes it.
struct PlayOnMenu: View {
    @ObservedObject var remote: RemoteModel
    let target: CastTarget
    /// "Shuffle on…" in it too (an album, an artist, a playlist).
    var shuffle = false
    /// Its accessibility identifier: "album" makes "album.playOn".
    var prefix = "item"
    /// Its symbol alone (the live player's overlay); VoiceOver reads its words.
    var iconOnly = false
    /// Told as a device is chosen: a sheet it is on closes, so the remote can show.
    var chosen: (() -> Void)?

    var body: some View {
        let devices = remote.takers(target.kind)
        if !devices.isEmpty {
            let names = remote.names
            let title = target.kind == .live ? Words.watchOn : Words.playOn
            Menu {
                ForEach(devices) { device in
                    choice(device, names: names, target: target)
                }
                if shuffle {
                    Menu(Words.shuffleOn) {
                        ForEach(devices) { device in
                            choice(device, names: names, target: target.shuffled)
                        }
                    }
                }
            } label: {
                if iconOnly {
                    // A target of its own, among the overlay's other buttons.
                    Label(title, systemImage: "play.tv").labelStyle(.iconOnly).frame(width: 44, height: 44)
                } else {
                    Label(title, systemImage: "play.tv")
                }
            }
            .accessibilityLabel(title)
            .accessibilityIdentifier("\(prefix).playOn")
        }
    }

    /// A device: its kind's symbol, its name, and what it plays now.
    private func choice(_ device: RemoteModel.Device, names: [String: String], target: CastTarget) -> some View {
        let name = names[device.id] ?? device.given ?? Words.unnamedDevice
        return Button {
            chosen?()
            Task { await remote.cast(target, to: device) }
        } label: {
            // A menu shows a Text after the Label as the item's second line.
            Label(name, systemImage: device.kind.symbol)
            Text(device.line)
        }
        .accessibilityIdentifier("\(prefix).playOn.\(device.id)")
    }
}
