#if os(iOS)
import PleyModel
import PleyScreens
import SwiftUI

/// The Library tab's picker (`LibraryPicker`): a short sheet over the tab
/// bar listing the profile's libraries and Live TV, the one showing
/// checked, then Edit order. Choosing one shows it under the tab and
/// closes the sheet. VoiceOver reads it as a menu of buttons.
struct LibraryPickerSheet: View {
    @ObservedObject var shell: ShellModel
    let entries: [LibraryPickerEntry]
    let done: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Libraries")
                .font(.footnote.weight(.bold))
                .textCase(.uppercase)
                .tracking(1.1)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .padding(.top, 20)
                .padding(.bottom, 4)
                .accessibilityAddTraits(.isHeader)
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(entries) { entry in
                        row(entry)
                    }
                    Divider().padding(.horizontal, 12).padding(.vertical, 6)
                    Button {
                        choose(.libraries)
                    } label: {
                        Label("Edit order", systemImage: "line.3.horizontal")
                            .font(.body)
                            .foregroundStyle(.primary)
                            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                            .padding(.horizontal, 12)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("library.picker.editOrder")
                }
            }
        }
        .padding(.horizontal, 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Libraries")
        .accessibilityIdentifier("library.picker")
    }

    private func row(_ entry: LibraryPickerEntry) -> some View {
        let showing = shell.section == entry.section
        return Button {
            choose(entry.section)
        } label: {
            HStack(spacing: 14) {
                Image(systemName: entry.symbol)
                    .font(.title3)
                    .foregroundStyle(.white)
                    .frame(width: 42, height: 42)
                    .background(Self.tint(entry), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                VStack(alignment: .leading, spacing: 1) {
                    Text(entry.title)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                    Text(entry.detail)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if entry.live {
                    HStack(spacing: 5) {
                        Circle().fill(Color.red).frame(width: 7, height: 7)
                        Text("Live")
                            .font(.caption.weight(.bold))
                            .textCase(.uppercase)
                            .foregroundStyle(.red)
                    }
                }
                if showing {
                    Image(systemName: "checkmark")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Color.accentColor)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(showing ? Color.accentColor.opacity(0.14) : .clear,
                        in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(entry.spoken)
        .accessibilityAddTraits(showing ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("library.picker.\(entry.id)")
    }

    private func choose(_ section: ShellSection) {
        shell.select(section)
        done()
    }

    /// Each kind's color: movies the accent, shows purple, music pink, Live TV red.
    private static func tint(_ entry: LibraryPickerEntry) -> Color {
        if entry.live { return .red }
        switch entry.symbol {
        case "tv": return .purple
        case "music.note": return .pink
        default: return .accentColor
        }
    }

    /// The sheet's height for `count` places: as tall as they need, up to most of the screen.
    static func height(_ count: Int) -> CGFloat {
        CGFloat(60 + 62 * count + 70)
    }
}
#endif
