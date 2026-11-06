import SwiftUI

/// Views in a line that wraps onto the next when it runs out of width, as
/// words do: a page's line of facts, its buttons at large text sizes, a
/// crew line's names. A separator (`flowSeparator()`) shows only between
/// two views on the same line: where the line breaks, it is left out, so
/// no line starts or ends with one.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8
    var lineSpacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        let lines = arrange(subviews, width: width)
        let height = lines.map(\.height).reduce(0, +) + lineSpacing * CGFloat(max(0, lines.count - 1))
        let widest = lines.map(\.width).max() ?? 0
        return CGSize(width: proposal.width ?? widest, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var placed = Set<Int>()
        var y = bounds.minY
        for line in arrange(subviews, width: bounds.width) {
            var x = bounds.minX
            for (index, size) in line.items {
                subviews[index].place(at: CGPoint(x: x, y: y + (line.height - size.height) / 2), anchor: .topLeading,
                                      proposal: ProposedViewSize(size))
                placed.insert(index)
                x += size.width + spacing
            }
            y += line.height + lineSpacing
        }
        // A separator left out at a break: no room, so nothing of it shows.
        for index in subviews.indices where !placed.contains(index) {
            subviews[index].place(at: bounds.origin, anchor: .topLeading, proposal: .zero)
        }
    }

    private struct Line {
        var items: [(Int, CGSize)] = []
        var width: CGFloat = 0
        var height: CGFloat = 0

        mutating func add(_ index: Int, _ size: CGSize, spacing: CGFloat) {
            width += (items.isEmpty ? 0 : spacing) + size.width
            height = max(height, size.height)
            items.append((index, size))
        }
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> [Line] {
        var lines: [Line] = []
        var line = Line()
        // A separator waits for the view after it: it goes on the line only
        // if that view does.
        var separator: (Int, CGSize)?
        for (index, view) in subviews.enumerated() {
            var size = view.sizeThatFits(.unspecified)
            size.width = min(size.width, width)
            if view[FlowSeparatorKey.self] {
                separator = line.items.isEmpty ? nil : (index, size)
                continue
            }
            if !line.items.isEmpty {
                let joined = separator.map { spacing + $0.1.width } ?? 0
                if line.width + joined + spacing + size.width > width {
                    lines.append(line)
                    line = Line()
                } else if let separator {
                    line.add(separator.0, separator.1, spacing: spacing)
                }
            }
            separator = nil
            line.add(index, size, spacing: spacing)
        }
        if !line.items.isEmpty { lines.append(line) }
        return lines
    }
}

private struct FlowSeparatorKey: LayoutValueKey {
    static let defaultValue = false
}

extension View {
    /// Marks a view in a `FlowLayout` as a separator, shown only between
    /// two views on the same line. Given no room, it shows nothing.
    func flowSeparator() -> some View {
        frame(minWidth: 0, maxWidth: .infinity, minHeight: 0, maxHeight: .infinity)
            .clipped()
            .layoutValue(key: FlowSeparatorKey.self, value: true)
    }
}
