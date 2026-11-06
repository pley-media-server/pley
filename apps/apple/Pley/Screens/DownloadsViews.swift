import ImageIO
import PleyData
import PleyScreens
import SwiftUI

/// Me → Downloads: the profile's downloads on this device, newest first,
/// each with where it is; a finished one plays without the server. A row
/// swiped (iPhone, iPad) or right-clicked (Mac) is deleted, or tried again.
struct DownloadsView: View {
    let app: AppModel
    @StateObject private var model: DownloadsModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: DownloadsModel(app: app))
    }

    var body: some View {
        Group {
            if model.rows.isEmpty {
                StateView(kind: .empty(symbol: "arrow.down.circle", text: DownloadWords.emptyTitle + ". "
                                       + DownloadWords.emptyMessage, action: nil))
            } else {
                List {
                    Section {
                        ForEach(model.rows) { row in
                            DownloadRowView(row: row) { Task { await model.play(row) } }
                                .contextMenu { actions(row) }
                                #if os(iOS)
                                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                    Button(role: .destructive) { model.remove(row) } label: {
                                        Label(row.canPlay ? DownloadWords.delete : DownloadWords.cancel,
                                              systemImage: "trash")
                                    }
                                }
                                .swipeActions(edge: .leading) {
                                    if row.failed {
                                        Button { model.retry(row) } label: {
                                            Label(DownloadWords.tryAgain, systemImage: "arrow.clockwise")
                                        }
                                        .tint(.accentColor)
                                    }
                                }
                                #endif
                        }
                    } footer: {
                        Text(model.storage)
                            .accessibilityIdentifier("downloads.storage")
                    }
                }
                #if os(macOS)
                .listStyle(.inset)
                #endif
            }
        }
        .navigationTitle(DownloadWords.meEntry)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .accessibilityIdentifier("downloads.page")
    }

    @ViewBuilder private func actions(_ row: DownloadRow) -> some View {
        if row.canPlay {
            Button { Task { await model.play(row) } } label: {
                Label(DownloadWords.play, systemImage: "play.fill")
            }
        }
        if row.failed {
            Button { model.retry(row) } label: {
                Label(DownloadWords.tryAgain, systemImage: "arrow.clockwise")
            }
        }
        if !app.offline {
            Button { app.shell.open(.item(row.entry.itemId)) } label: {
                Label(DownloadWords.goToPage, systemImage: "info.circle")
            }
        }
        Button(role: .destructive) { model.remove(row) } label: {
            Label(row.canPlay ? DownloadWords.delete : DownloadWords.cancel, systemImage: "trash")
        }
    }
}

/// A download in the list: its picture, title, episode or year, where it
/// is, and a bar while it comes. One element for VoiceOver, which reads
/// the rest; activating it plays a finished one.
private struct DownloadRowView: View {
    let row: DownloadRow
    let play: () -> Void

    var body: some View {
        Button(action: play) {
            HStack(spacing: 12) {
                LocalPicture(url: row.entry.image)
                    .frame(width: 96, height: 54)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(row.title).font(.body.weight(.semibold)).lineLimit(2)
                    if !row.subtitle.isEmpty {
                        Text(row.subtitle).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                    }
                    Text(row.status)
                        .font(.footnote)
                        .foregroundStyle(row.failed ? Color.red : Color.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    if let progress = row.progress {
                        ProgressView(value: progress)
                    }
                }
                Spacer(minLength: 0)
                if row.canPlay {
                    Image(systemName: "play.circle")
                        .font(.title2)
                        .foregroundStyle(Color.accentColor)
                        .accessibilityHidden(true)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!row.canPlay)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.accessibilityLabel)
        .accessibilityAddTraits(row.canPlay ? .isButton : [])
        .accessibilityHint(row.canPlay ? Text("Plays it") : Text(verbatim: ""))
        .accessibilityIdentifier("downloads.row.\(row.id)")
    }
}

/// A picture saved with a download, read off the main thread and made
/// small as it is read, so a long list stays light.
struct LocalPicture: View {
    let url: URL?
    @State private var image: CGImage?

    var body: some View {
        ZStack {
            Rectangle().fill(.quaternary)
            if let image {
                Image(decorative: image, scale: 1).resizable().scaledToFill()
            } else {
                Image(systemName: "film").foregroundStyle(.secondary)
            }
        }
        .task(id: url) {
            guard let url else { return }
            image = await Task.detached(priority: .utility) { Self.thumbnail(url, maxPixels: 320) }.value
        }
    }

    nonisolated static func thumbnail(_ url: URL, maxPixels: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true,
                                        kCGImageSourceThumbnailMaxPixelSize: maxPixels,
                                        kCGImageSourceCreateThumbnailWithTransform: true]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }
}

/// Settings → Downloads: the quality, mobile data (iPhone and iPad), what
/// they take, and Delete all downloads.
struct DownloadSettingsSections: View {
    @ObservedObject var model: DownloadsModel
    @State private var confirmDeleteAll = false

    var body: some View {
        Section {
            Picker("Quality", selection: Binding(get: { model.quality }, set: { model.quality = $0 })) {
                ForEach(DownloadQuality.allCases, id: \.self) { quality in
                    Text(quality.label).tag(quality)
                }
            }
            .accessibilityIdentifier("settings.downloads.quality")
            #if os(iOS)
            Toggle(isOn: Binding(get: { model.mobileData }, set: { model.mobileData = $0 })) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(DownloadWords.mobileData)
                    Text(DownloadWords.mobileDataHint).font(.footnote).foregroundStyle(.secondary)
                }
            }
            .accessibilityIdentifier("settings.downloads.mobileData")
            #endif
        } header: {
            Text("Downloads")
        } footer: {
            Text(DownloadWords.qualityHint)
        }
        Section {
            Button(DownloadWords.deleteAll, role: .destructive) { confirmDeleteAll = true }
                .disabled(model.rows.isEmpty)
                .accessibilityIdentifier("settings.downloads.deleteAll")
        } footer: {
            Text(model.storage)
        }
        .confirmationDialog(DownloadWords.deleteAllTitle, isPresented: $confirmDeleteAll, titleVisibility: .visible) {
            Button(DownloadWords.deleteAllConfirm, role: .destructive) { model.removeAll() }
                .accessibilityIdentifier("settings.downloads.deleteAll.confirm")
            Button(DownloadWords.keep, role: .cancel) {}
        } message: {
            Text(DownloadWords.deleteAllMessage)
        }
    }
}

/// Settings → Downloads on iPhone and iPad, a page of its own.
struct DownloadSettingsView: View {
    @StateObject private var model: DownloadsModel

    init(app: AppModel) {
        _model = StateObject(wrappedValue: DownloadsModel(app: app))
    }

    var body: some View {
        Form { DownloadSettingsSections(model: model) }
            .navigationTitle("Downloads")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
    }
}

/// The item page's Download button, as its download is: Download, where it
/// is with a bar, Downloaded (a menu to delete it), or why it stopped with
/// Try again.
struct DownloadButton: View {
    @ObservedObject var model: ItemModel

    var body: some View {
        if let state = model.downloadButton {
            switch state {
            case .download:
                Button {
                    Task { await model.download() }
                } label: {
                    Label(DownloadWords.download, systemImage: "arrow.down.circle")
                }
                .accessibilityIdentifier("item.download")
            case .progress(let status, let fraction):
                Menu {
                    Button(DownloadWords.cancel, role: .destructive) { model.removeDownload() }
                } label: {
                    HStack(spacing: 6) {
                        if let fraction {
                            ProgressView(value: fraction).frame(width: 44)
                        } else {
                            ProgressView().controlSize(.small)
                        }
                        Text(status).lineLimit(1)
                    }
                }
                .menuStyle(.button)
                .accessibilityLabel(status)
                .accessibilityIdentifier("item.downloading")
            case .downloaded:
                Menu {
                    Button(DownloadWords.delete, role: .destructive) { model.removeDownload() }
                } label: {
                    Label(DownloadWords.downloaded, systemImage: "checkmark.circle.fill")
                }
                .menuStyle(.button)
                .accessibilityIdentifier("item.downloaded")
            case .failed(let why):
                Menu {
                    Button(DownloadWords.tryAgain) { model.retryDownload() }
                    Button(DownloadWords.delete, role: .destructive) { model.removeDownload() }
                } label: {
                    Label(DownloadWords.tryAgain, systemImage: "exclamationmark.arrow.circlepath")
                }
                .menuStyle(.button)
                .accessibilityLabel(Text(verbatim: "\(why). \(DownloadWords.tryAgain)"))
                .accessibilityIdentifier("item.downloadFailed")
            }
        }
    }
}

/// "Pley is offline", above the signed-in app while the server can't be
/// reached, with the way to the downloads unless they are showing. The app
/// tries the server again by itself, and the banner goes when it answers.
struct OfflineBanner: View {
    @ObservedObject var app: AppModel
    @ObservedObject private var shell: ShellModel

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
    }

    var body: some View {
        if app.offline {
            HStack(spacing: 10) {
                Image(systemName: "wifi.slash").accessibilityHidden(true)
                Text(DownloadWords.offline)
                    .font(.subheadline)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                if app.downloads.available && shell.path(for: shell.section).last != .downloads {
                    Button(DownloadWords.offlineAction) { shell.open(.downloads) }
                        .font(.subheadline.weight(.semibold))
                        .accessibilityIdentifier("offline.downloads")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity)
            .background(.regularMaterial)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("offline.banner")
            .task { announce(DownloadWords.offline) }
        }
    }
}
