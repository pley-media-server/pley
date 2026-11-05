import PleyModel
import PleyScreens
import SwiftUI

// The pieces the request screens share: TMDB's posters with their badges,
// stage pills in their colours, the download's bar, the states a page
// can't show in, the count on the ways in, and TMDB's credit.

/// A TMDB title's poster, its status badge on it ("In library",
/// "Requested"), its title, and its year and kind under it. One element
/// for VoiceOver ("Dune: Part Two, 2024, Movie, Requested"); it opens the
/// title's page.
struct DiscoverCardView: View {
    let app: AppModel
    let card: DiscoverCard
    let width: CGFloat
    /// Its accessibility identifier: `discover.card.trending.movie:693134`.
    let identifier: String
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        Button {
            app.shell.open(card.screen)
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                ServerImage(app: app, path: card.posterPath, width: width,
                            symbol: card.source.kind == .show ? "tv" : "film")
                    .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
                    .overlay(alignment: .topLeading) {
                        if let badge = card.badge {
                            Text(badge)
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(.black.opacity(0.7), in: Capsule())
                                .padding(6)
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                VStack(alignment: .leading, spacing: 1) {
                    Text(card.title)
                        .font(.footnote.weight(.medium))
                        .lineLimit(typeSize.isAccessibilitySize ? 3 : 1)
                    Text(card.subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(typeSize.isAccessibilitySize ? 2 : 1)
                }
                .fixedSize(horizontal: false, vertical: true)
            }
            .frame(width: width, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        #if os(iOS)
        .hoverEffect(.lift)
        #endif
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(card.spoken)
        .accessibilityHint("Opens its page")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier(identifier)
    }
}

/// A request as a poster: its stage under its title, the download's
/// progress along the picture. It opens the title's page.
struct RequestTileView: View {
    let app: AppModel
    let card: RequestCard
    let width: CGFloat
    let identifier: String

    var body: some View {
        Button {
            if let screen = card.opens { app.shell.open(screen) }
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                ServerImage(app: app, path: card.posterPath, width: width,
                            symbol: card.request.kind == .show ? "tv" : "film")
                    .frame(width: width, height: Metrics.pictureHeight(.poster, width: width))
                    .overlay(alignment: .bottom) {
                        if let progress = card.pill.progress { ProgressBar(fraction: progress) }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                VStack(alignment: .leading, spacing: 1) {
                    Text(card.title).font(.footnote.weight(.medium)).lineLimit(1)
                    Text(card.pill.text).font(.caption).foregroundStyle(PillColors.of(card.pill.tone)).lineLimit(2)
                }
            }
            .frame(width: width, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(card.opens == nil)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(card.spoken)
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier(identifier)
    }
}

/// A stage pill's colours: waiting for approval amber; on its way blue;
/// ready green; declined and failed red; gone gray.
enum PillColors {
    static func of(_ tone: PillTone) -> Color {
        switch tone {
        case .amber: .orange
        case .blue: .accentColor
        case .green: .green
        case .red: .red
        case .gray: .secondary
        }
    }

    static func fill(_ tone: PillTone) -> Color {
        switch tone {
        case .amber: .orange.opacity(0.14)
        case .blue: .accentColor.opacity(0.16)
        case .green: .green.opacity(0.16)
        case .red: .red.opacity(0.16)
        case .gray: .secondary.opacity(0.14)
        }
    }
}

/// A request's stage: its words beside a dot in the stage's colour, the
/// download's bar under it while one runs (gray while paused, red with a
/// problem), and the line a failed request has.
struct StagePillView: View {
    let pill: PillWords

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Circle().fill(PillColors.of(pill.tone)).frame(width: 6, height: 6)
                Text(pill.text)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(PillColors.of(pill.tone))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(PillColors.fill(pill.tone), in: RoundedRectangle(cornerRadius: 6))
            .accessibilityElement(children: .combine)
            if let progress = pill.progress {
                DownloadBar(progress: progress, paused: pill.paused, warning: pill.warning)
                    .frame(maxWidth: 240)
            }
            if let line = pill.line {
                Text(line).font(.footnote).foregroundStyle(.secondary)
            }
        }
    }
}

/// A download's progress, 4 points tall; VoiceOver hears its percentage.
struct DownloadBar: View {
    let progress: Double
    var paused = false
    var warning = false

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Rectangle().fill(.quaternary)
                Rectangle().fill(warning ? Color.red : paused ? Color.secondary : Color.accentColor)
                    .frame(width: geometry.size.width * min(1, max(0, progress)))
            }
        }
        .frame(height: 4)
        .clipShape(Capsule())
        .accessibilityElement()
        .accessibilityLabel("Download")
        .accessibilityValue("\(Int((min(1, max(0, progress)) * 100).rounded())) percent")
    }
}

/// Why a page can't show: requests off, no access, sign in again (with
/// Sign in), TMDB's key refused, TMDB away or the server out of
/// reach (with Retry), a title not found. VoiceOver reads it out when it
/// shows. Its buttons' identifiers start with `prefix`: `discover.retry`.
struct ProblemView: View {
    let words: ProblemWords
    let prefix: String
    var retry: (() -> Void)?
    var signInAgain: (() -> Void)?

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: words.symbol)
                .font(.largeTitle)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text(words.title).font(.headline)
            if let body = words.body {
                Text(body).foregroundStyle(.secondary)
            }
            if words.signInAgain, let signInAgain {
                Button(RequestWords.signIn, action: signInAgain)
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("\(prefix).signInAgain")
            } else if words.retry, let retry {
                Button(action: retry) {
                    Label(RequestWords.retry, systemImage: "arrow.clockwise")
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("\(prefix).retry")
            }
        }
        .multilineTextAlignment(.center)
        .padding()
        .frame(maxWidth: 480)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(prefix).problem")
        .task(id: words) { announce([words.title, words.body].compactMap { $0 }.joined(separator: " ")) }
    }
}

/// Reading again failed: what shows is from before, and why, with Retry where it may help.
struct StaleLine: View {
    let words: ProblemWords
    let retry: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.circle").foregroundStyle(.secondary).accessibilityHidden(true)
            Text([words.title, words.body].compactMap { $0 }.joined(separator: " "))
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            if words.retry {
                Button(RequestWords.retry, action: retry)
                    .font(.footnote)
                    .accessibilityIdentifier("stale.retry")
            }
        }
        .accessibilityElement(children: .contain)
    }
}

/// The number on the ways into requests: the accent's colour, red while
/// failed requests wait for a manager. Its words are on what holds it.
struct CountBadge: View {
    let text: String
    let alarm: Bool

    var body: some View {
        Text(text)
            .font(.caption2.bold().monospacedDigit())
            .foregroundStyle(.white)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .frame(minWidth: 20)
            .background(alarm ? Color.red : Color.accentColor, in: Capsule())
            .accessibilityHidden(true)
    }
}

/// "Details and pictures from TMDB.", at the foot of Discover and of every title page.
struct TmdbCredit: View {
    var body: some View {
        Text(RequestWords.credit)
            .font(.footnote)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .accessibilityIdentifier("tmdb.credit")
    }
}

/// A filter chip: its words in a capsule, filled while chosen.
struct ChipButton: View {
    let title: String
    let selected: Bool
    let identifier: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline.weight(selected ? .semibold : .regular))
                .foregroundStyle(selected ? Color.white : Color.primary)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(selected ? Color.accentColor : Color.secondary.opacity(0.15), in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }
}
