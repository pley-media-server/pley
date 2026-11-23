import PleyModel
import PleyScreens
import SwiftUI

/// The buttons of Home's banner, for the focus.
enum HomeHeroButton: Hashable {
    case play, myList, details
}

/// Home's banner on Apple TV (the iPhone's, Android TV's): one featured
/// title at a time, over its backdrop behind the whole of Home, with why it
/// is there ("New in Movies"), its title, facts and summary, and Play
/// (Resume while part played), My List and Details. Home opens with the
/// focus on Play. Left from Play and right from Details turn to the title
/// before and after; it turns by itself every 9 s while it shows, except
/// under Reduce Motion. Once the focus goes down to the rows, the banner
/// gives its room to the info block and its words hide, but its buttons
/// stay where they are, so up from the first row comes back to it.
struct HomeHero: View {
    let app: AppModel
    let titles: [FeaturedTitle]
    /// The title showing: kept by Home, so it stays while the rows have the focus.
    @Binding var index: Int
    /// Whether the banner shows, rather than the info block over the rows.
    let shows: Bool
    var focus: FocusState<HomeHeroButton?>.Binding
    @ObservedObject private var actions: ItemActionsModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(app: AppModel, titles: [FeaturedTitle], index: Binding<Int>, shows: Bool,
         focus: FocusState<HomeHeroButton?>.Binding) {
        self.app = app
        self.titles = titles
        _index = index
        self.shows = shows
        self.focus = focus
        _actions = ObservedObject(wrappedValue: app.actions)
    }

    /// The banner's height while it shows: the first row peeks out below it.
    static let height: CGFloat = 560

    private var current: FeaturedTitle { titles[min(index, titles.count - 1)] }

    var body: some View {
        let title = current
        VStack(alignment: .leading, spacing: 0) {
            // Gone, not only unseen, while the rows have the focus: the buttons
            // then sit at the bottom of the info block's room, just above the rows.
            if shows { words(title) }
            Spacer(minLength: 0)
            buttons(title)
                .opacity(shows ? 1 : 0)
            dots
                .opacity(shows && titles.count > 1 ? 1 : 0)
                .padding(.top, 24)
        }
        .padding(.horizontal, Metrics.gutter)
        .padding(.top, 20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(Motion.animation(.easeInOut(duration: 0.35), reduced: reduceMotion), value: index)
        .task(id: "\(index) \(shows) \(titles.map(\.id))") { await turnLater() }
        .focusSection()
        .accessibilityIdentifier("home.hero")
    }

    private func words(_ title: FeaturedTitle) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title.kicker)
                .font(.caption.weight(.bold))
                .textCase(.uppercase)
                .tracking(1.6)
                .foregroundStyle(Color.accentColor)
            Text(title.info.title)
                .font(.largeTitle.bold())
                .lineLimit(2)
                .minimumScaleFactor(0.7)
            if !title.info.meta.isEmpty {
                Text(title.info.meta.joined(separator: " · "))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if !title.info.summary.isEmpty {
                Text(title.info.summary)
                    .font(.callout)
                    .lineLimit(3)
                    .padding(.top, 4)
            }
        }
        .frame(maxWidth: 1000, alignment: .leading)
        // Readable over the bright parts of a backdrop the scrim doesn't darken enough.
        .shadow(color: .black.opacity(0.6), radius: 8)
        // Another title's words fade in, at once under Reduce Motion.
        .id(title.id)
        .transition(.opacity)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("home.hero.title")
    }

    private func buttons(_ title: FeaturedTitle) -> some View {
        let play: ItemAction = title.inProgress ? .resume : .play
        let listed = actions.listed?.contains(title.id) == true
        let listAction: ItemAction = listed ? .removeFromMyList : .addToMyList
        return HStack(spacing: 24) {
            Button {
                ItemActionsMenu.run(app, play, on: title.item)
            } label: {
                Label(play.title, systemImage: "play.fill")
            }
            .focused(focus, equals: .play)
            .onMoveCommand { if $0 == .left { turn(by: -1) } }
            // VoiceOver says which title Play plays, and which of how many it is.
            .accessibilityValue(Text(verbatim: title.info.title))
            .accessibilityHint(titles.count > 1 ? Text("\(index + 1) of \(titles.count)") : Text(verbatim: ""))
            .accessibilityIdentifier("home.hero.play")
            Button {
                ItemActionsMenu.run(app, listAction, on: title.item)
            } label: {
                Image(systemName: listed ? "checkmark" : "plus")
            }
            .focused(focus, equals: .myList)
            .accessibilityLabel(listAction.title)
            .accessibilityIdentifier("home.hero.myList")
            Button {
                app.shell.open(item: title.item)
            } label: {
                Image(systemName: "info.circle")
            }
            .focused(focus, equals: .details)
            .onMoveCommand { if $0 == .right { turn(by: 1) } }
            .accessibilityLabel("Details")
            .accessibilityIdentifier("home.hero.details")
        }
    }

    /// Which title shows, of how many.
    private var dots: some View {
        HStack(spacing: 10) {
            ForEach(titles.indices, id: \.self) { at in
                Capsule()
                    .fill(at == index ? Color.primary : Color.secondary.opacity(0.5))
                    .frame(width: at == index ? 28 : 10, height: 10)
            }
        }
        .accessibilityHidden(true)
    }

    private func turn(by step: Int) {
        guard titles.count > 1 else { return }
        index = (index + step + titles.count) % titles.count
    }

    /// The next title after `Featured.turnSeconds` while the banner shows,
    /// except under Reduce Motion. The wait starts over with each turn.
    private func turnLater() async {
        guard shows, !reduceMotion, titles.count > 1 else { return }
        try? await Task.sleep(for: .seconds(Featured.turnSeconds))
        guard !Task.isCancelled else { return }
        turn(by: 1)
    }
}
