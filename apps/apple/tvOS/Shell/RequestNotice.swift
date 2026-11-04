import Combine
import PleyModel
import PleyScreens
import SwiftUI
import UIKit

/// Requests' news over the pages, one at a time, as on Android TV: on a
/// TV, what became ready to watch, the only news the app tells there. A
/// card at the bottom right with the title's poster (or a green tick
/// without one), the news, Open and Close.
///
/// It shows once the remote has rested `rest` (Pley coming back to the
/// front, and the player closing, count as a press), with no player open,
/// nothing presented over the shell (a panel, a sheet, a question, the
/// keyboard), and neither the Search tab's keyboard nor the requests list
/// showing; there the news is already in sight. On screen it is off the
/// queue (`noticeShown`), so newer news can't push it out. It takes the
/// focus, on Open; VoiceOver reads it out. Open opens the title where it
/// is in a library, else the requests. Close, Menu, or moving up or down
/// out of it close it, as do `showsFor` untouched; the focus goes back to
/// the page. A Select in its first moments was meant for the page: it does
/// nothing.
struct RequestNoticeView: View {
    @ObservedObject var app: AppModel
    @ObservedObject private var shell: ShellModel
    @StateObject private var remote = RemoteRest()
    @State private var shown: RequestNews?

    /// How long the remote rests before news shows.
    static let rest: Duration = .milliseconds(1500)
    /// How long it shows untouched.
    static let showsFor: Duration = .seconds(10)

    init(app: AppModel) {
        self.app = app
        _shell = ObservedObject(wrappedValue: app.shell)
    }

    var body: some View {
        ZStack {
            if let news = shown {
                NoticeCard(app: app, news: news, remote: remote) { open in
                    shown = nil
                    // Waits for the remote to rest from this press too.
                    remote.touched()
                    if open { app.shell.open(AppModel.opens(news)) }
                }
                .id(news)
            }
        }
        .task(id: waiting) { await wait() }
        // Back from the player counts as a press: a film that ran to its
        // end untouched doesn't bring news up the moment it closes.
        .onChange(of: app.player == nil && app.livePlayer == nil) { _, closed in
            if closed { remote.touched() }
        }
    }

    /// What starts the wait again: the next news, or the card closing.
    private var waiting: RequestNews? { shown == nil ? app.notices.first : nil }

    /// Shows the first news once the remote has rested and the screen is free for it.
    private func wait() async {
        guard let next = waiting else { return }
        while !Task.isCancelled {
            let quiet = remote.quietFor
            if quiet >= Self.rest && free {
                app.noticeShown(next)
                shown = next
                return
            }
            try? await Task.sleep(for: quiet < Self.rest ? Self.rest - quiet : .milliseconds(250))
        }
    }

    /// Nothing that holds the remote is up: the players, a panel, a sheet,
    /// a question or the keyboard (all presented over the shell), Search's
    /// keyboard, or the requests list, which shows the news itself.
    private var free: Bool {
        guard app.player == nil, app.livePlayer == nil, shell.sheet == nil else { return false }
        guard UIApplication.shared.applicationState == .active, !RemoteRest.presenting else { return false }
        let section = shell.section
        let path = shell.path(for: section)
        if section == .search && path.isEmpty { return false }
        if let top = path.last { return top != .requests }
        return !(section == .requests && !shell.requestsEntry.discover)
    }
}

/// The card: 600 points wide, the poster at 80 by 120, the news in three
/// lines at most, then Open and Close.
private struct NoticeCard: View {
    let app: AppModel
    let news: RequestNews
    let remote: RemoteRest
    let close: (_ open: Bool) -> Void
    @FocusState private var focus: Control?
    @State private var closing = false
    @State private var armed = false
    @State private var hadFocus = false
    @State private var touches = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var visible = false

    private enum Control: Hashable {
        case open, close
    }

    private var text: String { RequestWords.notice(news) }

    private var poster: String? {
        if case .ready(_, _, _, _, _, _, let poster) = news { return poster }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            HStack(alignment: .center, spacing: 24) {
                if let poster {
                    ServerImage(app: app, path: poster, width: 80, symbol: "film")
                        .frame(width: 80, height: 120)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                        .accessibilityHidden(true)
                } else {
                    // The poster says which title it is; without one, a tick says it's good news.
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 44))
                        .foregroundStyle(.green)
                        .accessibilityHidden(true)
                }
                Text(text)
                    .font(.callout)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("notice.text")
            }
            HStack(spacing: 30) {
                Button("Open") { if armed { finish(open: true) } }
                    .focused($focus, equals: .open)
                    .accessibilityIdentifier("notice.open")
                Button(RequestWords.close) { if armed { finish(open: false) } }
                    .focused($focus, equals: .close)
                    .accessibilityIdentifier("notice.close")
            }
        }
        .padding(32)
        .frame(width: 600, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20))
        .padding(.trailing, Metrics.gutter)
        .padding(.bottom, 60)
        .opacity(visible ? 1 : 0)
        .focusSection()
        .onExitCommand { finish(open: false) }
        .onMoveCommand { direction in
            touches += 1
            if direction == .up || direction == .down { finish(open: false) }
        }
        .onChange(of: focus) { _, now in
            touches += 1
            if now != nil {
                hadFocus = true
            } else if hadFocus {
                // The focus went to the page: the card has done its work.
                finish(open: false)
            }
        }
        .task {
            withAnimation(reduceMotion ? nil : .easeOut(duration: 0.15)) { visible = true }
            announce(text)
            await Task.yield()
            focus = .open
            try? await Task.sleep(for: .milliseconds(600))
            armed = true
        }
        // Untouched, it goes by itself; every press starts its time again.
        .task(id: touches) {
            try? await Task.sleep(for: RequestNoticeView.showsFor)
            if !Task.isCancelled { finish(open: false) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("notice")
    }

    private func finish(open: Bool) {
        guard !closing else { return }
        closing = true
        close(open)
    }
}

/// When the remote last moved the focus, or Pley came back to the front:
/// news waits for the remote to rest.
@MainActor
final class RemoteRest: ObservableObject {
    private var last = ContinuousClock.now
    private var listeners: Set<AnyCancellable> = []

    init() {
        let center = NotificationCenter.default
        center.publisher(for: UIFocusSystem.didUpdateNotification)
            .merge(with: center.publisher(for: UIApplication.didBecomeActiveNotification))
            .sink { [weak self] _ in MainActor.assumeIsolated { self?.touched() } }
            .store(in: &listeners)
    }

    var quietFor: Duration { ContinuousClock.now - last }

    func touched() { last = .now }

    /// Something is presented over the app: a panel, a sheet, a question, the keyboard, the player.
    static var presenting: Bool {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .contains { $0.isKeyWindow && $0.rootViewController?.presentedViewController != nil }
    }
}
