import PleyData
import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// Home's banner (Android's phone banner, the web's): one featured title at
/// a time over its backdrop, which fades into the page below it, with why
/// it is there ("New in Movies"), its title and facts, and Play (Resume
/// while part played), My List and Details. It turns to the next title
/// every 9 s, except while it is touched, while it is off the screen, and
/// under Reduce Motion. A swipe, or a dot, turns it by hand; VoiceOver
/// turns it with a swipe up or down on the title, and reads which of how
/// many it is.
struct HomeHero: View {
    let app: AppModel
    let titles: [FeaturedTitle]
    /// The title showing: kept by Home, so it stays while the banner is scrolled away.
    @Binding var index: Int
    @ObservedObject private var actions: ItemActionsModel
    @State private var touching = false
    private let layout = WideLayout()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    init(app: AppModel, titles: [FeaturedTitle], index: Binding<Int>) {
        self.app = app
        self.titles = titles
        _index = index
        _actions = ObservedObject(wrappedValue: app.actions)
    }

    private var current: FeaturedTitle { titles[min(index, titles.count - 1)] }

    var body: some View {
        GeometryReader { geometry in
            let title = current
            ZStack(alignment: .bottomLeading) {
                ServerImage(app: app, paths: [title.backdrop], width: geometry.size.width, symbol: nil,
                            placeholder: false, limit: ImageLoader.deviceBackdropLimit)
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .clipped()
                    .id(title.id)
                    .transition(.opacity)
                fade
                words(title)
                    .padding(.horizontal, Metrics.gutter(regular: layout.wide))
                    .padding(.bottom, 16)
            }
        }
        .frame(height: height)
        .background(Color.secondary.opacity(0.15))
        .clipShape(RoundedRectangle(cornerRadius: layout.wide ? Metrics.heroCorner : 0, style: .continuous))
        .padding(.horizontal, layout.wide ? Metrics.gutter(regular: true) : 0)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.35), value: index)
        #if os(iOS)
        .simultaneousGesture(swipe)
        #endif
        .task(id: "\(index) \(touching) \(titles.map(\.id))") { await turnLater() }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Featured")
        .accessibilityIdentifier("home.hero")
    }

    /// Tall on a phone, wide and lower beside the iPad's and the Mac's sidebar.
    private var height: CGFloat { layout.wide ? 420 : 500 }

    /// The picture fades into the page, under the words.
    private var fade: some View {
        LinearGradient(stops: [.init(color: Self.page.opacity(0), location: 0.3),
                               .init(color: Self.page.opacity(0.85), location: 0.68),
                               .init(color: Self.page, location: 1)],
                       startPoint: .top, endPoint: .bottom)
            .accessibilityHidden(true)
    }

    /// The page's own background, which the picture fades into.
    static var page: Color {
        #if os(macOS)
        Color(nsColor: .windowBackgroundColor)
        #else
        Color(uiColor: .systemBackground)
        #endif
    }

    private func words(_ title: FeaturedTitle) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title.kicker)
                    .font(.caption.weight(.bold))
                    .textCase(.uppercase)
                    .tracking(1.2)
                    .foregroundStyle(Color.accentColor)
                Text(title.info.title)
                    .font(.largeTitle.bold())
                    .lineLimit(2)
                    .minimumScaleFactor(0.7)
                if !title.info.meta.isEmpty {
                    Text(title.info.meta.joined(separator: " · "))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityValue(titles.count > 1 ? Text("\(index + 1) of \(titles.count)") : Text(verbatim: ""))
            .accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: turn(by: 1)
                case .decrement: turn(by: -1)
                @unknown default: break
                }
            }
            .accessibilityIdentifier("home.hero.title")
            buttons(title)
                .padding(.top, 6)
            if titles.count > 1 {
                dots
                    .frame(maxWidth: .infinity)
                    .padding(.top, 4)
            }
        }
        .frame(maxWidth: layout.wide ? 560 : .infinity, alignment: .leading)
        .id(title.id)
        .transition(.opacity)
    }

    private func buttons(_ title: FeaturedTitle) -> some View {
        let play: ItemAction = title.inProgress ? .resume : .play
        let listed = actions.listed?.contains(title.id) == true
        let listAction: ItemAction = listed ? .removeFromMyList : .addToMyList
        return HStack(spacing: 10) {
            Button {
                run(play, on: title.item)
            } label: {
                Label(play.title, systemImage: "play.fill")
                    .font(.headline)
                    .frame(maxWidth: layout.wide ? nil : .infinity, minHeight: 34)
                    .padding(.horizontal, layout.wide ? 24 : 0)
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("home.hero.play")
            Button {
                run(listAction, on: title.item)
            } label: {
                Image(systemName: listed ? "checkmark" : "plus")
                    .font(.headline)
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel(listAction.title)
            .accessibilityIdentifier("home.hero.myList")
            Button {
                app.shell.open(item: title.item)
            } label: {
                Image(systemName: "info.circle")
                    .font(.headline)
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Details")
            .accessibilityIdentifier("home.hero.details")
        }
        .controlSize(.large)
    }

    /// Which title shows, of how many; each dot turns to its title.
    private var dots: some View {
        HStack(spacing: 6) {
            ForEach(titles.indices, id: \.self) { at in
                Button {
                    index = at
                } label: {
                    Capsule()
                        .fill(at == index ? Color.primary : Color.secondary.opacity(0.5))
                        .frame(width: at == index ? 18 : 6, height: 6)
                        .padding(.vertical, 8)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        // VoiceOver turns the banner from its title instead.
        .accessibilityHidden(true)
    }

    #if os(iOS)
    /// A swipe sideways turns to the title before or after; the banner waits while touched.
    private var swipe: some Gesture {
        DragGesture(minimumDistance: 20)
            .onChanged { _ in touching = true }
            .onEnded { value in
                touching = false
                let across = value.translation.width
                guard abs(across) > abs(value.translation.height), abs(across) > 48 else { return }
                turn(by: across < 0 ? 1 : -1)
            }
    }
    #endif

    private func turn(by step: Int) {
        guard titles.count > 1 else { return }
        index = (index + step + titles.count) % titles.count
    }

    /// The next title after `Featured.turnSeconds`, unless touched or under Reduce Motion.
    /// The wait starts over with each turn, and stops while the banner is off the screen.
    private func turnLater() async {
        guard !reduceMotion, !touching, titles.count > 1 else { return }
        try? await Task.sleep(for: .seconds(Featured.turnSeconds))
        guard !Task.isCancelled else { return }
        turn(by: 1)
    }

    private func run(_ action: ItemAction, on item: ItemSummary) {
        #if os(macOS)
        ItemActionsMenu.run(app, action, on: item) { openWindow(id: PlayerWindow.id, value: $0) }
        #else
        ItemActionsMenu.run(app, action, on: item, play: nil)
        #endif
    }
}
