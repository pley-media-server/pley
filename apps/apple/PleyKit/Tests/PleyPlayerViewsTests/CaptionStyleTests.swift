import CoreGraphics
import PleyData
@testable import PleyPlayerViews
import Testing

/// Settings' subtitle choices over the system's caption style.
@Suite struct CaptionStyleTests {
    /// A caption style as the system might set one: a yellow box, a font, larger text.
    private var system: CaptionStyle {
        var style = CaptionStyle()
        style.background = CaptionStyle.RGBA(1, 1, 0, 0.5)
        style.fontName = "Courier"
        style.scale = 1.5
        style.edge = .shadow
        return style
    }

    @Test func systemEverywhereKeepsTheSystemsStyle() {
        let merged = system.merged(SubtitleOverrides())
        #expect(merged == system)
        #expect(merged.raise == 0)
    }

    @Test func eachChoiceReplacesWhatTheSystemSet() {
        let merged = system.merged(SubtitleOverrides(size: .p75, background: .none, font: .serif, raise: .fifteen))
        #expect(merged.scale == 0.75)
        #expect(merged.background == nil)
        #expect(merged.family == .serif)
        #expect(merged.fontName == nil)
        #expect(abs(merged.raise - 0.15) < 0.0001)
        // What Settings has no choice for stays the system's.
        #expect(merged.edge == .shadow)
        #expect(merged.text == system.text)
    }

    @Test func aDarkBoxIsPutBehindPleysOwnLook() {
        let merged = CaptionStyle().merged(SubtitleOverrides(size: .p200, background: .darkBox))
        #expect(merged.background == CaptionStyle.darkBox)
        #expect(merged.scale == 2)
        #expect(merged.family == .system)
    }

    @Test func aChosenTypefaceIsUsed() {
        let sans = CaptionStyle().merged(SubtitleOverrides(font: .sansSerif)).font(size: 20, bold: true, italic: false)
        let mono = CaptionStyle().merged(SubtitleOverrides(font: .monospace)).font(size: 20, bold: true, italic: false)
        #expect(sans.pointSize == 20)
        #expect(mono.fontName != sans.fontName)
        if SubtitleFont.casualAvailable {
            let casual = CaptionStyle().merged(SubtitleOverrides(font: .casual))
                .font(size: 20, bold: true, italic: false)
            #expect(casual.fontName.contains("Noteworthy"))
        } else {
            #expect(!SubtitleFont.offered.contains(.casual))
        }
    }
}
