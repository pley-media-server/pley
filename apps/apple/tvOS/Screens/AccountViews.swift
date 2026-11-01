import PleyModel
import PleyScreens
import SwiftUI

/// History on Apple TV: the profile's plays, newest first, 15 at a time
/// with Show more. Select opens a play's page; a long press removes it.
struct HistoryView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: HistoryModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: HistoryModel(app: app))
    }

    var body: some View {
        content
            .task { await model.load() }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed(let why):
            VStack(spacing: 24) {
                Text("Couldn't load your history. \(why)").foregroundStyle(.secondary)
                Button("Try again") { Task { await model.load() } }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .loaded where model.rows.isEmpty:
            // Focusable, so the focus stays on the page and Menu goes back
            // to Settings rather than up to the tabs.
            Text(HistoryModel.emptyText)
                .font(.title3)
                .foregroundStyle(.secondary)
                .focusable()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityIdentifier("history.empty")
        case .loaded:
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 24) {
                    Text("History").font(.title2.bold()).accessibilityAddTraits(.isHeader)
                    ForEach(model.rows) { row in
                        HistoryCard(app: app, row: row) { app.shell.open(row.opens) }
                            .contextMenu {
                                Button("Remove from History", role: .destructive) {
                                    Task { await model.remove(row.id) }
                                }
                            }
                    }
                    if model.canLoadMore {
                        Button(model.loadingMore ? "Loading…" : "Show more") { Task { await model.loadMore() } }
                            .disabled(model.loadingMore)
                            .accessibilityIdentifier("history.more")
                    }
                }
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, 40)
            }
            .focusSection()
            .accessibilityIdentifier("history.list")
        }
    }
}

/// A play as a wide button: its picture, title, what it was, and when and where it played.
private struct HistoryCard: View {
    let app: AppModel
    let row: HistoryRow
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 32) {
                ServerImage(app: app, paths: row.pictures, width: 100, symbol: Metrics.symbol(row.kind))
                    .frame(width: 100, height: row.square ? 100 : 150)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                VStack(alignment: .leading, spacing: 6) {
                    Text(row.title).font(.headline).lineLimit(1)
                    if !row.subtitle.isEmpty {
                        Text(row.subtitle).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Text([row.time, row.device].compactMap { $0 }.joined(separator: " · "))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
            }
            .padding(12)
            .frame(maxWidth: .infinity)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.spoken)
        .accessibilityHint("Opens its page. Hold to remove it.")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("history.row.\(row.id)")
    }
}

/// Diagnostics on Apple TV: the device's class and what it plays, and the
/// report the app sends with each playback request. The report scrolls
/// with the remote.
struct DiagnosticsView: View {
    @StateObject private var model: DiagnosticsModel

    init(app: AppModel) {
        _model = StateObject(wrappedValue: DiagnosticsModel(app: app))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Text("Diagnostics").font(.title2.bold()).accessibilityAddTraits(.isHeader)
                Text("Device class: \(model.deviceClass)").font(.callout)
                ForEach(model.lines, id: \.self) { line in
                    Text(line).font(.caption)
                }
                Text("What Pley sends with each playback request").font(.headline).padding(.top, 20)
                // Lines of their own, each focusable, so the remote scrolls through them.
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(model.json.split(separator: "\n").enumerated()), id: \.offset) { _, line in
                        Text(String(line))
                            .font(.system(.caption2, design: .monospaced))
                            .focusable()
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("diagnostics.json")
            }
            .padding(.horizontal, Metrics.gutter)
            .padding(.vertical, 40)
        }
        .task { await model.load() }
    }
}

/// About on Apple TV: the version, that the app holds no third-party code, and Pley's licence.
struct AboutView: View {
    private let model = AboutModel()

    var body: some View {
        VStack(alignment: .leading, spacing: 32) {
            Text("Pley").font(.title.bold())
            Text(model.title)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("about.version")
            Text(AboutModel.licences)
            Text(AboutModel.licence)
        }
        // One focusable block, so the focus stays on the page and Menu goes
        // back to Settings.
        .focusable()
        .padding(.horizontal, Metrics.gutter)
        .frame(maxWidth: 1400, maxHeight: .infinity, alignment: .topLeading)
    }
}
