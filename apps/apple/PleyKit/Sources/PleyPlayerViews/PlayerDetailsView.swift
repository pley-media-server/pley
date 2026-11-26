import PleyModel
import PleyPlayer
import SwiftUI

/// How the video plays, for Details: the way ("Direct stream"), the
/// quality and why Automatic went lower, why the server didn't play the
/// file as it is, what the screen gets of HDR video, what the server's
/// conversion makes, and the subtitles shown (and whether they are burned in). Apple TV shows it as an info panel tab, the other devices in the
/// Options menu (iPhone and iPad) or the action menu (Mac).
public struct PlayerDetailsView: View {
    let state: PlayerState

    public init(state: PlayerState) {
        self.state = state
    }

    public var body: some View {
        DetailLines(lines: PlayerDetails.lines(state))
    }
}

/// How a Live TV channel plays, for the live player's Details, in the
/// look of the player's: the way ("Direct stream"), the
/// provider's video and audio and what the server makes of them, what the
/// provider sends now, and the encoder of a transcode.
public struct LiveDetailsView: View {
    let plan: LivePlan?

    public init(plan: LivePlan?) {
        self.plan = plan
    }

    public var body: some View {
        DetailLines(lines: LiveDetails.lines(plan))
    }
}

/// Details' lines, each one's title over its text.
struct DetailLines: View {
    let lines: [PlayerDetails.Line]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(lines, id: \.self) { line in
                VStack(alignment: .leading, spacing: 2) {
                    Text(line.title).font(.caption).foregroundStyle(.secondary)
                    Text(line.text)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Details as lines of a title and a text, for every platform's menu.
public enum PlayerDetails {
    public struct Line: Hashable, Sendable {
        public var title: String
        public var text: String
    }

    public static func lines(_ state: PlayerState) -> [Line] {
        var lines: [Line] = []
        lines.append(Line(title: localized("Playing"), text: state.methodLabel ?? localized("Starting…")))
        lines.append(Line(title: localized("Quality"), text: state.quality))
        if let note = state.qualityNote {
            lines.append(Line(title: localized("Lowered"), text: note))
        }
        if !state.reasons.isEmpty {
            lines.append(Line(title: localized("Why"), text: state.reasons.joined(separator: " ")))
        }
        if let range = state.videoRange, !range.isEmpty {
            lines.append(Line(title: localized("Video"), text: range))
        }
        if let out = state.videoOut, !out.isEmpty {
            lines.append(Line(title: localized("Video out"), text: out))
        }
        if state.subtitles.contains(where: { $0.selected && $0.id != MenuChoice.off }) {
            let label = state.subtitleLabel
            let text = state.subtitlesBurnedIn ? localized("\(label) · burned into the video") : label
            lines.append(Line(title: localized("Subtitles"), text: text))
        }
        return lines
    }
}

/// A Live TV channel's details as lines, like the player's, from its
/// session's plan (`GET /api/live/sessions/{session}/plan`). Apple's
/// players take Live TV as the server's HLS stream alone (AVPlayer plays
/// no MPEG-TS as it comes), so a channel plays as a direct stream while
/// the server copies its video, or has none, and as a transcode otherwise,
/// in the player's words (`PlayerState.label(of:audioConverted:)`: a
/// direct stream says when its audio is converted); nothing says why, as
/// nothing here could have played it another way.
public enum LiveDetails {
    /// The lines of `plan`: how it plays, then what is known of the video,
    /// the audio, the bitrate and the encoder. Without a plan (the server
    /// doesn't know it yet), how it plays alone, as the player says before
    /// its server answered.
    public static func lines(_ plan: LivePlan?) -> [PlayerDetails.Line] {
        guard let plan else {
            return [PlayerDetails.Line(title: localized("Playing"), text: localized("Starting…"))]
        }
        let converted = isConverted(plan)
        let method = PlayerState.label(of: converted ? .transcode : .remux, audioConverted: audioConverted(plan))
        var lines = [PlayerDetails.Line(title: localized("Playing"), text: method)]
        if let video = video(plan, converted: converted) {
            lines.append(PlayerDetails.Line(title: localized("Video"), text: video))
        }
        if let audio = audio(plan) {
            lines.append(PlayerDetails.Line(title: localized("Audio"), text: audio))
        }
        if let bitrate = bitrate(plan, converted: converted) {
            lines.append(PlayerDetails.Line(title: localized("Bitrate"), text: bitrate))
        }
        if converted {
            lines.append(PlayerDetails.Line(title: localized("Encoder"), text: encoderName(plan.video)))
        }
        return lines
    }

    /// Whether the server converts the video: `video` names its encoder,
    /// not `copy` or `none` (a channel of sound alone).
    static func isConverted(_ plan: LivePlan) -> Bool {
        !["", "copy", "none"].contains(plan.video.lowercased())
    }

    /// Whether the server converts the audio: it makes AAC stereo of any
    /// sound the HLS stream can't carry as it is.
    static func audioConverted(_ plan: LivePlan) -> Bool {
        plan.audio.lowercased() == "aac"
    }

    /// FFmpeg's encoders by the names people know, as the other apps say them.
    private static let encoders = [
        "libx264": "x264", "libx265": "x265",
        "h264_nvenc": "NVIDIA NVENC", "hevc_nvenc": "NVIDIA NVENC",
        "h264_qsv": "Intel Quick Sync", "hevc_qsv": "Intel Quick Sync",
        "h264_amf": "AMD AMF", "hevc_amf": "AMD AMF",
        "h264_vaapi": "VA-API", "hevc_vaapi": "VA-API",
        "h264_videotoolbox": "VideoToolbox", "hevc_videotoolbox": "VideoToolbox",
        "h264_vulkan": "Vulkan", "hevc_vulkan": "Vulkan",
        "h264_v4l2m2m": "V4L2",
    ]

    /// An encoder by the name people know: "h264_nvenc" is "NVIDIA NVENC",
    /// "libx264" "x264"; one not known as it is.
    static func encoderName(_ encoder: String) -> String {
        encoders[encoder] ?? encoder
    }

    /// "HEVC 10-bit · 3840×2160 · 50 fps · interlaced → H.264 1080p": the
    /// provider's video, then what a transcode makes of it (its lines the
    /// channel's own unless the plan says otherwise); `nil` without video.
    private static func video(_ plan: LivePlan, converted: Bool) -> String? {
        var name = Words.codec(plan.videoCodec)
        guard !name.isEmpty else { return nil }
        if let depth = plan.bitDepth, depth > 8 {
            name += " " + localized("\(depth)-bit")
        }
        var parts = [name]
        if let width = plan.width, let height = plan.height, width > 0, height > 0 {
            parts.append("\(width)×\(height)")
        }
        if let rate = plan.frameRate, rate > 0, rate < 1000 {
            let number = rate.rounded() == rate ? String(Int(rate)) : Words.fixed(rate, digits: 2)
            parts.append(localized("\(number) fps"))
        }
        if plan.interlaced {
            parts.append(localized("interlaced"))
        }
        var text = parts.joined(separator: " · ")
        if converted {
            var out: [String] = []
            if let codec = encoded(plan.video) {
                out.append(codec)
            }
            if let made = plan.outHeight ?? plan.height, made > 0 {
                out.append("\(made)p")
            }
            text += " → " + (out.isEmpty ? "…" : out.joined(separator: " "))
        }
        return text
    }

    /// The codec an encoder makes, by FFmpeg's name for it ("h264_nvenc",
    /// "libx264"): "H.264"; `nil` for one whose name doesn't say.
    static func encoded(_ encoder: String) -> String? {
        let name = encoder.lowercased()
        if name.contains("264") { return "H.264" }
        if name.contains("265") || name.contains("hevc") { return "HEVC" }
        if name.contains("av1") { return "AV1" }
        return nil
    }

    /// "EAC3 5.1 → AAC Stereo": the provider's audio, then the AAC stereo
    /// the server makes of any other; `nil` without audio.
    private static func audio(_ plan: LivePlan) -> String? {
        let codec = Words.codec(plan.audioCodec)
        guard !codec.isEmpty else { return nil }
        let text = [codec, Words.channels(plan.audioChannels)].filter { !$0.isEmpty }.joined(separator: " ")
        guard audioConverted(plan) else { return text }
        return text + " → " + Words.codec("aac") + " " + Words.channels(2)
    }

    /// "12 Mbps → 8 Mbps": what the provider sends now, then a
    /// transcode's video bitrate; `nil` until the server has measured it.
    private static func bitrate(_ plan: LivePlan, converted: Bool) -> String? {
        let sent = Words.bitrate(bps(plan.kbps))
        guard !sent.isEmpty else { return nil }
        let made = converted ? Words.bitrate(bps(plan.videoKbps)) : ""
        return made.isEmpty ? sent : sent + " → " + made
    }

    /// kbit/s as bit/s; `nil` for none, or for more than any stream's.
    private static func bps(_ kbps: Int?) -> Int? {
        guard let kbps, kbps > 0, kbps < 10_000_000 else { return nil }
        return kbps * 1000
    }
}
