import AVFoundation
import CoreGraphics
import CoreText
import Foundation
import MediaAccessibility
import PleyData
import PleySubtitles
#if canImport(UIKit)
import UIKit
typealias PlatformFont = UIFont
typealias PlatformFontDescriptor = UIFontDescriptor
typealias PlatformColor = UIColor
typealias DrawingOptions = NSStringDrawingOptions
#else
import AppKit
typealias PlatformFont = NSFont
typealias PlatformFontDescriptor = NSFontDescriptor
typealias PlatformColor = NSColor
typealias DrawingOptions = NSString.DrawingOptions
#endif

/// How text subtitles look. Pley's own look is the web's and Android's:
/// white, bold, a dark outline and no box, 4.8 % of the video's height.
/// When the person chose a caption style in the system's settings
/// (Accessibility → Subtitles & Captioning), what that style sets wins:
/// colours, background, font, size and edges. What they chose in Pley's
/// Settings (size, background, font, position) wins over both.
public struct CaptionStyle: Sendable, Equatable {
    public enum Edge: Sendable {
        case none, outline, shadow, raised, depressed
    }

    /// The text's height as a share of the video's.
    public static let share: CGFloat = 0.048
    /// The space under bottom subtitles (and over top ones), as a share of the video's height, as on the web.
    public static let margin: CGFloat = 0.06

    public var text: RGBA = RGBA(1, 1, 1, 1)
    /// A box behind the text; `nil` for none.
    public var background: RGBA?
    /// The system's font for captions; `nil`: the system's bold.
    public var fontName: String?
    /// The size the style asks for, against pley's: 1 is 4.8 % of the height.
    public var scale: CGFloat = 1
    public var edge: Edge = .outline
    /// A typeface chosen in Settings, over `fontName`; `.system` for none.
    public var family: SubtitleFont = .system
    /// How far bottom text subtitles are raised, a share of the video's height.
    public var raise: CGFloat = 0

    /// A colour as four components (sRGB), so the style stays a value.
    public struct RGBA: Sendable, Equatable {
        public var red, green, blue, alpha: CGFloat

        public init(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat, _ alpha: CGFloat) {
            (self.red, self.green, self.blue, self.alpha) = (red, green, blue, alpha)
        }

        init?(_ color: CGColor, opacity: CGFloat) {
            guard let srgb = CGColorSpace(name: CGColorSpace.sRGB),
                  let converted = color.converted(to: srgb, intent: .defaultIntent, options: nil),
                  let c = converted.components, c.count >= 3
            else { return nil }
            self.init(c[0], c[1], c[2], (c.count > 3 ? c[3] : 1) * opacity)
        }

        var color: PlatformColor { PlatformColor(red: red, green: green, blue: blue, alpha: alpha) }
    }

    public init() {}

    /// The box Settings' "Dark box" puts behind the text.
    public static let darkBox = RGBA(0, 0, 0, 0.75)

    /// Pley's look, with what the system's caption style sets and, over it,
    /// what Settings sets.
    public static func current(overrides: SubtitleOverrides = AppSettings().subtitles) -> CaptionStyle {
        system().merged(overrides)
    }

    /// The style with Settings' choices over it: a size replaces the
    /// system's, None takes the box away and Dark box puts one in, a
    /// typeface replaces the system's, and the raise moves text up.
    public func merged(_ overrides: SubtitleOverrides) -> CaptionStyle {
        var style = self
        if let scale = overrides.size.scale { style.scale = CGFloat(scale) }
        switch overrides.background {
        case .system: break
        case .none: style.background = nil
        case .darkBox: style.background = Self.darkBox
        }
        if overrides.font != .system {
            style.family = overrides.font
            style.fontName = nil
        }
        style.raise = CGFloat(overrides.raise.share)
        return style
    }

    /// Pley's look, with what the system's caption style sets.
    public static func system() -> CaptionStyle {
        var style = CaptionStyle()
        var behavior = MACaptionAppearanceBehavior.useContentIfAvailable
        let text = MACaptionAppearanceCopyForegroundColor(.user, &behavior).takeRetainedValue()
        if behavior == .useValue {
            let opacity = MACaptionAppearanceGetForegroundOpacity(.user, nil)
            if let rgba = RGBA(text, opacity: opacity) { style.text = rgba }
        }
        behavior = .useContentIfAvailable
        let back = MACaptionAppearanceCopyBackgroundColor(.user, &behavior).takeRetainedValue()
        if behavior == .useValue {
            let opacity = MACaptionAppearanceGetBackgroundOpacity(.user, nil)
            style.background = RGBA(back, opacity: opacity).flatMap { $0.alpha > 0.01 ? $0 : nil }
        }
        behavior = .useContentIfAvailable
        let font = MACaptionAppearanceCopyFontDescriptorForStyle(.user, &behavior, .default).takeRetainedValue()
        if behavior == .useValue {
            style.fontName = CTFontDescriptorCopyAttribute(font, kCTFontNameAttribute) as? String
        }
        behavior = .useContentIfAvailable
        let scale = MACaptionAppearanceGetRelativeCharacterSize(.user, &behavior)
        if behavior == .useValue, scale > 0 { style.scale = min(max(scale, 0.5), 2) }
        behavior = .useContentIfAvailable
        let edge = MACaptionAppearanceGetTextEdgeStyle(.user, &behavior)
        if behavior == .useValue {
            style.edge = switch edge {
            case .none: .none
            case .dropShadow: .shadow
            case .raised: .raised
            case .depressed: .depressed
            default: .outline
            }
        }
        return style
    }

    /// The font for text `size` points tall, bold or not, italic or not.
    func font(size: CGFloat, bold: Bool, italic: Bool) -> PlatformFont {
        let weight: PlatformFont.Weight = bold ? .bold : .semibold
        var base = chosenFont(size: size, weight: weight)
            ?? fontName.flatMap { PlatformFont(name: $0, size: size) }
            ?? PlatformFont.systemFont(ofSize: size, weight: weight)
        if italic || bold && (fontName != nil || family == .casual) {
            var traits = CTFontGetSymbolicTraits(base as CTFont)
            if italic { traits.insert(.traitItalic) }
            if bold { traits.insert(.traitBold) }
            if let styled = CTFontCreateCopyWithSymbolicTraits(base as CTFont, size, nil, traits, traits) {
                base = styled as PlatformFont
            }
        }
        return base
    }

    /// The typeface chosen in Settings; `nil` for the system's choice.
    private func chosenFont(size: CGFloat, weight: PlatformFont.Weight) -> PlatformFont? {
        let system = PlatformFont.systemFont(ofSize: size, weight: weight)
        switch family {
        case .system: return nil
        case .sansSerif: return system
        case .serif: return Self.designed(system, .serif, size: size)
        case .monospace: return Self.designed(system, .monospaced, size: size)
        case .casual: return PlatformFont(name: SubtitleFont.casualName, size: size)
        }
    }

    private static func designed(_ font: PlatformFont, _ design: PlatformFontDescriptor.SystemDesign,
                                 size: CGFloat) -> PlatformFont? {
        guard let descriptor = font.fontDescriptor.withDesign(design) else { return nil }
        return PlatformFont(descriptor: descriptor, size: size)
    }
}

/// Draws subtitles over the video, into a flipped Core Graphics context (y
/// down): text cues centred at the bottom (or the top, for those placed in
/// the upper half), pictures where their plane puts them, scaled to the
/// video's rectangle.
enum SubtitleDrawing {
    /// Where the video shows: `video` when known, else a picture of
    /// `aspect` fitted into `bounds`, else all of `bounds`.
    static func area(_ video: CGRect, aspect: CGSize, in bounds: CGRect) -> CGRect {
        if !video.isEmpty { return video }
        guard aspect.width > 0, aspect.height > 0 else { return bounds }
        return AVMakeRect(aspectRatio: aspect, insideRect: bounds)
    }

    static func draw(_ shown: ShownSubtitles, in context: CGContext, video: CGRect, style: CaptionStyle) {
        guard video.width > 1, video.height > 1 else { return }
        switch shown {
        case .none: return
        case .text(let cues):
            let size = max(video.height * CaptionStyle.share * style.scale, 12)
            let bottom = cues.filter { !$0.top }, top = cues.filter(\.top)
            if !bottom.isEmpty { drawText(bottom, atTop: false, size: size, video: video, style: style) }
            if !top.isEmpty { drawText(top, atTop: true, size: size, video: video, style: style) }
        case .picture(let picture):
            drawPicture(picture.frame, in: context, video: video)
        }
    }

    private static func drawText(_ cues: [TextCue], atTop: Bool, size: CGFloat, video: CGRect, style: CaptionStyle) {
        let text = NSMutableAttributedString()
        for (n, cue) in cues.enumerated() {
            if n > 0 { text.append(NSAttributedString(string: "\n")) }
            text.append(attributed(cue.text, size: size, style: style))
        }
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        text.addAttribute(.paragraphStyle, value: paragraph, range: NSRange(location: 0, length: text.length))
        let width = video.width * 0.9
        let options: DrawingOptions = [.usesLineFragmentOrigin, .usesFontLeading]
        let measured = text.boundingRect(with: CGSize(width: width, height: .greatestFiniteMagnitude),
                                         options: options, context: nil)
        let height = ceil(measured.height)
        let margin = video.height * CaptionStyle.margin
        // Settings' raise lifts subtitles at the bottom; pictures keep their place.
        let y = atTop ? video.minY + margin : video.maxY - margin - video.height * style.raise - height
        let rect = CGRect(x: video.midX - width / 2, y: y, width: width, height: height)
        if let background = style.background {
            let used = ceil(measured.width) + size * 0.6
            background.color.setFill()
            fill(CGRect(x: video.midX - used / 2, y: y - size * 0.15, width: used, height: height + size * 0.3))
        }
        switch style.edge {
        case .outline:
            // The outline first, under the letters: a stroke of the glyphs, then the text filled over it.
            let outline = NSMutableAttributedString(attributedString: text)
            let all = NSRange(location: 0, length: outline.length)
            outline.addAttribute(.strokeWidth, value: 16, range: all)
            outline.addAttribute(.strokeColor, value: PlatformColor(white: 0, alpha: style.text.alpha * 0.9),
                                 range: all)
            outline.draw(with: rect, options: options, context: nil)
        case .shadow, .raised, .depressed:
            let shadow = NSShadow()
            shadow.shadowColor = PlatformColor(white: 0, alpha: 0.9)
            shadow.shadowBlurRadius = style.edge == .shadow ? size * 0.12 : 0
            let lift = size * (style.edge == .shadow ? 0.06 : 0.04)
            shadow.shadowOffset = CGSize(width: lift, height: style.edge == .depressed ? -lift : lift)
            text.addAttribute(.shadow, value: shadow, range: NSRange(location: 0, length: text.length))
        case .none:
            break
        }
        text.draw(with: rect, options: options, context: nil)
    }

    /// A cue's text in the style's font and colour, with its italics, bold and underlining.
    private static func attributed(_ text: AttributedString, size: CGFloat, style: CaptionStyle) -> NSAttributedString {
        let out = NSMutableAttributedString()
        for run in text.runs {
            let intent = run.inlinePresentationIntent ?? []
            var attributes: [NSAttributedString.Key: Any] = [
                .font: style.font(size: size, bold: true, italic: intent.contains(.emphasized)),
                .foregroundColor: style.text.color,
            ]
            if run[SubtitleUnderline.self] == true { attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue }
            out.append(NSAttributedString(string: String(text[run.range].characters), attributes: attributes))
        }
        return out
    }

    private static func drawPicture(_ frame: PictureFrame, in context: CGContext, video: CGRect) {
        guard frame.planeWidth > 0, frame.planeHeight > 0 else { return }
        let sx = video.width / CGFloat(frame.planeWidth), sy = video.height / CGFloat(frame.planeHeight)
        for bitmap in frame.bitmaps {
            guard let image = image(bitmap) else { continue }
            let rect = CGRect(x: video.minX + CGFloat(bitmap.x) * sx, y: video.minY + CGFloat(bitmap.y) * sy,
                              width: CGFloat(bitmap.width) * sx, height: CGFloat(bitmap.height) * sy)
            // Core Graphics draws images bottom up: flipped back around the picture's place.
            context.saveGState()
            context.interpolationQuality = .high
            context.translateBy(x: 0, y: rect.maxY)
            context.scaleBy(x: 1, y: -1)
            context.draw(image, in: CGRect(x: rect.minX, y: 0, width: rect.width, height: rect.height))
            context.restoreGState()
        }
    }

    /// The bitmap as an image: its pixels are RGBA, not premultiplied.
    static func image(_ bitmap: SubtitleBitmap) -> CGImage? {
        guard bitmap.width > 0, bitmap.height > 0, bitmap.rgba.count >= bitmap.width * bitmap.height * 4,
              let provider = CGDataProvider(data: Data(bitmap.rgba) as CFData),
              let space = CGColorSpace(name: CGColorSpace.sRGB)
        else { return nil }
        return CGImage(width: bitmap.width, height: bitmap.height, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: bitmap.width * 4, space: space,
                       bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue), provider: provider,
                       decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }

    private static func fill(_ rect: CGRect) {
        #if canImport(UIKit)
        UIRectFill(rect)
        #else
        rect.fill()
        #endif
    }
}

/// What the system says when the person changes the caption style.
let captionStyleChanged = Notification.Name(kMACaptionAppearanceSettingsChangedNotification as String)

#if canImport(UIKit)
/// The subtitles over the video, in `AVPlayerViewController`'s content
/// overlay: above the picture, under the controls. Touches go through.
final class SubtitleView: UIView {
    var shown: ShownSubtitles = .none {
        didSet { if shown != oldValue { setNeedsDisplay() } }
    }
    /// Where the video shows, in this view's coordinates; empty: from `aspect`.
    var video: CGRect = .zero {
        didSet { if video != oldValue { setNeedsDisplay() } }
    }
    /// The picture's size, fitted into the view where `video` isn't known
    /// (Apple TV's player doesn't say); zero: all of the view.
    var aspect: CGSize = .zero {
        didSet { if aspect != oldValue { setNeedsDisplay() } }
    }
    private var style = CaptionStyle.current()

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        isUserInteractionEnabled = false
        contentMode = .redraw
        autoresizingMask = [.flexibleWidth, .flexibleHeight]
        NotificationCenter.default.addObserver(self, selector: #selector(captionStyleDidChange),
                                               name: captionStyleChanged, object: nil)
        // Settings' subtitle style, changed while playing (the Mac's Settings window).
        NotificationCenter.default.addObserver(self, selector: #selector(captionStyleDidChange),
                                               name: UserDefaults.didChangeNotification, object: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not from a nib") }

    /// The person changed the system's caption style, or Settings': drawn again with it.
    @objc private func captionStyleDidChange() {
        Task { @MainActor [weak self] in
            guard let self else { return }
            let style = CaptionStyle.current()
            guard style != self.style else { return }
            self.style = style
            self.setNeedsDisplay()
        }
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        SubtitleDrawing.draw(shown, in: context, video: SubtitleDrawing.area(video, aspect: aspect, in: bounds),
                             style: style)
    }
}
#else
/// The subtitles over the video, in `AVPlayerView`'s content overlay:
/// above the picture, under the controls. Clicks go through.
final class SubtitleView: NSView {
    var shown: ShownSubtitles = .none {
        didSet { if shown != oldValue { needsDisplay = true } }
    }
    /// Where the video shows, in this view's coordinates; empty: all of it.
    var video: CGRect = .zero {
        didSet { if video != oldValue { needsDisplay = true } }
    }
    private var style = CaptionStyle.current()

    override init(frame: NSRect) {
        super.init(frame: frame)
        autoresizingMask = [.width, .height]
        NotificationCenter.default.addObserver(self, selector: #selector(captionStyleDidChange),
                                               name: captionStyleChanged, object: nil)
        // Settings' subtitle style, changed while playing (the Mac's Settings window).
        NotificationCenter.default.addObserver(self, selector: #selector(captionStyleDidChange),
                                               name: UserDefaults.didChangeNotification, object: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not from a nib") }

    /// The person changed the system's caption style, or Settings': drawn again with it.
    @objc private func captionStyleDidChange() {
        Task { @MainActor [weak self] in
            guard let self else { return }
            let style = CaptionStyle.current()
            guard style != self.style else { return }
            self.style = style
            self.needsDisplay = true
        }
    }

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { false }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        SubtitleDrawing.draw(shown, in: context, video: video.isEmpty ? bounds : video, style: style)
    }
}
#endif
