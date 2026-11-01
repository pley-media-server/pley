import Foundation
import PleyCaps
import PleyModel

/// How a playback is asked for: which file, from where, with which tracks,
/// and what goes in the body of `POST /api/playback`. A port of the Android
/// app's `PlaybackPlan`; pure: it reads the item, the caps report and the
/// choices, nothing else.
///
/// The audio is the server's to choose unless the person picked a track:
/// it knows the profile's language and, from the caps, which of that
/// language's tracks this device plays best, and says which one plays
/// (`PlaybackInfo.audioIndex`). Track indexes go to the server only when it
/// has to act on them:
/// - `audio_index` for a track picked, when it can't be played here, the
///   session streams HLS (one audio track), or it isn't the track the
///   item's details give;
/// - `subtitle_index` only to burn picture subtitles into HLS, and never
///   when this player side-loads them.
public enum PlaybackPlan {
    /// The file to play: the one asked for, else the one that plays best here, else the first.
    public static func file(_ item: ItemDetail, requested: Int?, caps: ClientCaps?) -> MediaFile? {
        if let requested, let file = item.files.first(where: { $0.id == requested }) { return file }
        if let caps, let best = CapsPolicy.bestFile(item, caps), let file = item.files.first(where: { $0.id == best }) {
            return file
        }
        return item.files.first
    }

    /// Where to start: as asked, else 0 when asked from the start or watched, else the resume point.
    public static func startMs(_ item: ItemDetail, _ request: PlayRequest) -> Int {
        if let start = request.startMs { return max(start, 0) }
        if request.fromStart || item.userState.watched { return 0 }
        return max(item.userState.positionMs, 0)
    }

    /// The audio to start with: the track asked for, else `nil`, which
    /// lets the server pick the one this device plays best.
    public static func initialAudio(_ file: MediaFile?, requested: Int?) -> MediaStream? {
        guard let requested else { return nil }
        return file?.streams(.audio).first { $0.index == requested }
    }

    /// The audio `file` plays with track `index` as a session's answer names
    /// it: that track, else the file's default, else the first.
    public static func playedAudio(_ file: MediaFile?, index: Int?) -> MediaStream? {
        let audio = file?.streams(.audio) ?? []
        return audio.first { $0.index == index } ?? audio.first(where: \.default) ?? audio.first
    }

    /// The audio the item's details give: the profile's choice, else the
    /// file's default, else the first.
    public static func detailsAudio(_ file: MediaFile?) -> MediaStream? {
        let audio = file?.streams(.audio) ?? []
        return audio.first { $0.index == file?.selectedAudio } ?? audio.first(where: \.default) ?? audio.first
    }

    /// Subtitles shown as pictures (PGS, VobSub, DVB): a stream burns them into the video.
    public static func isPicture(_ stream: MediaStream?) -> Bool { stream?.textBased == false }

    /// Whether this device decodes `stream`'s audio (nothing is passed through on Apple's devices).
    public static func audioPlaysHere(_ stream: MediaStream?, caps: ClientCaps?) -> Bool {
        guard let stream, let caps else { return true }
        let codec = stream.codec.lowercased()
        let known = caps.audioCodecs + (caps.audioDecode ?? []) + (caps.audioPassthrough ?? [])
        return known.contains { $0.lowercased() == codec }
    }

    /// Whether this device opens `file`'s container as it is.
    public static func containerPlaysHere(_ file: MediaFile?, caps: ClientCaps?) -> Bool {
        guard let container = file?.container?.lowercased(), let caps else { return true }
        let containers = caps.containers.map { $0.lowercased() }
        return containers.contains(container)
            || (container == "mkv" && containers.contains("matroska"))
            || (container == "matroska" && containers.contains("mkv"))
    }

    /// Whether the session will surely stream (HLS), before asking: another
    /// mode than auto, a video this device doesn't play as it is, a
    /// container it doesn't open, or audio picked that it can't play.
    public static func streams(mode: PlaybackMode, decision: CapsDecision?, file: MediaFile?, audio: MediaStream?,
                               caps: ClientCaps?) -> Bool {
        mode == .remux || mode == .transcode
            || (decision.map { $0 != .direct } ?? false)
            || !containerPlaysHere(file, caps: caps)
            || !audioPlaysHere(audio, caps: caps)
    }

    /// What a request carries besides the item: the choices of the moment.
    public struct Choices: Hashable, Sendable {
        public var file: MediaFile
        public var startMs: Int
        /// `nil`: the server picks the track.
        public var audio: MediaStream?
        public var subtitle: MediaStream?
        /// kbps and lines of a quality below the original; `nil` for the original.
        public var maxBitrate: Int?
        public var maxHeight: Int?
        public var mode: PlaybackMode
        /// The session will stream (HLS): the server needs the track indexes.
        public var streaming: Bool
        /// Pictures are side-loaded by the player: none to burn in.
        public var sideload: Bool

        public init(file: MediaFile, startMs: Int, audio: MediaStream? = nil, subtitle: MediaStream? = nil,
                    maxBitrate: Int? = nil, maxHeight: Int? = nil, mode: PlaybackMode = .auto,
                    streaming: Bool = false, sideload: Bool = false) {
            self.file = file
            self.startMs = startMs
            self.audio = audio
            self.subtitle = subtitle
            self.maxBitrate = maxBitrate
            self.maxHeight = maxHeight
            self.mode = mode
            self.streaming = streaming
            self.sideload = sideload
        }
    }

    /// The body of `POST /api/playback` for `choices`: keyed URLs always,
    /// since AVPlayer sends no token. `keepPrevious` when it is made ready
    /// while the session before plays on.
    public static func request(itemId: Int, _ choices: Choices, clientId: String, caps: ClientCaps?,
                               keepPrevious: Bool = false) -> PlaybackRequest {
        let audioHere = audioPlaysHere(choices.audio, caps: caps)
        let notTheDetails = choices.audio.map { $0.index != detailsAudio(choices.file)?.index } ?? false
        let audioIndex = choices.audio.flatMap { audio in
            choices.streaming || !audioHere || notTheDetails ? audio.index : nil
        }
        let subtitleIndex = choices.subtitle.flatMap { sub in
            choices.streaming && isPicture(sub) && !choices.sideload ? sub.index : nil
        }
        return PlaybackRequest(itemId: itemId, fileId: choices.file.id, audioIndex: audioIndex,
                               subtitleIndex: subtitleIndex, maxBitrate: choices.maxBitrate,
                               maxHeight: choices.maxHeight, startMs: choices.startMs, mode: choices.mode,
                               clientId: clientId, keepPrevious: keepPrevious ? true : nil, keyedUrls: true,
                               caps: caps ?? ClientCaps())
    }

    /// Whether `info`'s session must be asked again with the track indexes:
    /// it streams though it wasn't expected to, and its audio isn't the one
    /// picked, or the subtitles picked are pictures to burn in.
    public static func needsIndexes(_ info: PlaybackInfo, sent: PlaybackRequest, _ choices: Choices) -> Bool {
        guard info.hls else { return false }
        let wrongAudio = choices.audio.map { audio in
            sent.audioIndex == nil && info.audioIndex != nil && info.audioIndex != audio.index
        } ?? false
        let missingBurn = isPicture(choices.subtitle) && sent.subtitleIndex == nil && !choices.sideload
        return wrongAudio || missingBurn
    }
}
