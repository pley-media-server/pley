import Foundation
import PleyModel
@testable import PleyPlayer
import PleyTesting
import Testing

/// The cases of the Android app's MarkersTest and NextUpTest (and the web's SkipButton.test.tsx).
@Suite("Skip Intro, Skip Credits and what comes next")
struct MarkersTests {
    private func marker(_ kind: MarkerKind, _ start: Int, _ end: Int, file: Int = 1) -> Marker {
        Marker(fileId: file, kind: kind, startMs: start, endMs: end, source: "chapter")
    }

    private var intro: Marker { marker(.intro, 10_000, 70_000) }
    private var credits: Marker { marker(.credits, 1_200_000, 1_300_000) }

    @Test func takesTheMarkersOfTheFileBeingPlayedElseTheFirstFile() {
        let other = marker(.intro, 0, 5_000, file: 2)
        let item = ItemDetail(summary: ItemSummary(id: 1, libraryId: 1, kind: .movie, title: "A"),
                              files: [MediaFile(id: 1, path: "a.mkv"), MediaFile(id: 2, path: "b.mkv")],
                              markers: [intro, other, marker(MarkerKind(rawValue: "recap"), 0, 1_000)])
        #expect(Markers.forFile(item, fileId: 2) == [other])
        #expect(Markers.forFile(item, fileId: nil) == [intro])
        #expect(Markers.forFile(ItemDetail(summary: item.summary, files: item.files), fileId: 1).isEmpty)
    }

    @Test func findsTheSegmentThePlayheadIsInUntilASecondBeforeItEnds() {
        let markers = [intro, credits]
        #expect(Markers.active(markers, positionMs: 9_900) == nil)
        #expect(Markers.active(markers, positionMs: 10_000) == intro)
        #expect(Markers.active(markers, positionMs: 68_900) == intro)
        #expect(Markers.active(markers, positionMs: 69_000) == nil)
        #expect(Markers.active(markers, positionMs: 1_250_000) == credits)
        #expect(Markers.label(intro) == "Skip Intro")
        #expect(Markers.label(credits) == "Skip Credits")
    }

    @Test func landsJustBeforeAnIntroEndsAndAtTheEndOfCredits() {
        #expect(Markers.skipTarget(intro) == 69_250)
        #expect(Markers.skipTarget(credits) == 1_300_000)
        // Inside a very short intro.
        #expect(Markers.skipTarget(marker(.intro, 0, 1_000)) == 500)
    }

    @Test func announcesTheCreditsOnceEvenWhenSeekingBackIntoThem() {
        var gate = CreditsGate()
        #expect(gate.entered(nil) == nil)
        #expect(gate.entered(intro) == nil)
        #expect(gate.entered(credits) == credits)
        #expect(gate.entered(credits) == nil)
        // Other credits (another file's, or another segment) are announced.
        let other = marker(.credits, 1_200_000, 1_300_000, file: 2)
        #expect(gate.entered(other) == other)
        gate.reset()
        #expect(gate.entered(credits) == credits)
    }

    @Test func theNextOneStartsWhereItResumesUnlessWatched() {
        var next = ItemSummary(id: 12, libraryId: 1, kind: .episode, title: "Two")
        next.parentIndex = 1
        next.index = 2
        next.userState = UserState(positionMs: 90_000)
        #expect(NextUp.startOf(next) == 90_000)
        next.userState = UserState(positionMs: 90_000, watched: true)
        #expect(NextUp.startOf(next) == 0)
        #expect(NextUp.heading(next) == "Up next · S1 E2")
        #expect(NextUp.heading(ItemSummary(id: 13, libraryId: 1, kind: .movie, title: "M")) == "Up next")
        #expect(NextUp.hasNext(.episode))
        #expect(!NextUp.hasNext(.movie))
    }
}

/// The Android app's track mapping, for AVPlayer's audible group.
@Suite("Audio tracks")
struct TrackChoicesTests {
    private func file() -> MediaFile {
        var english = MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true)
        english.channels = 2
        var french = MediaStream(index: 2, kind: .audio, codec: "ac3", language: "fre")
        french.channels = 6
        var commentary = MediaStream(index: 3, kind: .audio, codec: "aac", language: "eng")
        commentary.title = "Commentary"
        commentary.channels = 2
        return MediaFile(id: 70, path: "a.mp4", container: "mp4",
                         streams: [MediaStream(index: 0, kind: .video, codec: "h264"), english, french, commentary])
    }

    @Test func theMenuNamesEachTrackAndMarksTheOnePlaying() {
        let menu = TrackChoices.audioMenu(file(), playing: 2)
        #expect(menu.map(\.id) == ["1", "2", "3"])
        #expect(menu.map(\.label) == ["English (AAC Stereo)", "French (AC3 5.1)", "English – Commentary (AAC Stereo)"])
        #expect(menu.map(\.selected) == [false, true, false])
        // Nothing named: the default.
        #expect(TrackChoices.audioMenu(file(), playing: nil).map(\.selected) == [true, false, false])
    }

    @Test func aTrackIsTheOptionAtItsPlaceCheckedByLanguage() {
        let options = [EngineAudioOption(position: 0, language: "en"), EngineAudioOption(position: 1, language: "fr"),
                       EngineAudioOption(position: 2, language: "en")]
        #expect(TrackChoices.option(for: 2, in: file(), options: options)?.position == 1)
        #expect(TrackChoices.option(for: 3, in: file(), options: options)?.position == 2)
        #expect(TrackChoices.stream(for: options[1], in: file(), options: options)?.index == 2)
        // The order disagrees: the one option of its language, when only one has it.
        let shifted = [EngineAudioOption(position: 0, language: "fra"), EngineAudioOption(position: 1, language: "en")]
        #expect(TrackChoices.option(for: 2, in: file(), options: shifted)?.position == 0)
        // Two English options and the order disagrees: not surely either.
        let unsure = [EngineAudioOption(position: 0, language: "fre"), EngineAudioOption(position: 1, language: "en"),
                      EngineAudioOption(position: 2, language: "en")]
        #expect(TrackChoices.option(for: 1, in: file(), options: unsure) == nil)
        // Without languages, the place alone; nothing at all without options.
        let bare = [EngineAudioOption(position: 0, language: nil), EngineAudioOption(position: 1, language: "und")]
        #expect(TrackChoices.option(for: 2, in: file(), options: bare)?.position == 1)
        #expect(TrackChoices.option(for: 2, in: file(), options: []) == nil)
        #expect(TrackChoices.option(for: 9, in: file(), options: options) == nil)
    }

    @Test func languagesMatchInEitherSpelling() {
        #expect(TrackChoices.normalized("en-US") == "eng")
        #expect(TrackChoices.normalized("fra") == "fre")
        #expect(TrackChoices.normalized("fre") == "fre")
        #expect(TrackChoices.normalized("und") == nil)
        #expect(TrackChoices.normalized(nil) == nil)
    }

    @Test func theTracksStartAsAskedElseAsTheServerPicks() {
        let f = file()
        #expect(PlaybackPlan.initialAudio(f, requested: 2)?.index == 2)
        #expect(PlaybackPlan.initialAudio(f, requested: nil) == nil)
        #expect(PlaybackPlan.playedAudio(f, index: 2)?.index == 2)
        #expect(PlaybackPlan.playedAudio(f, index: nil)?.index == 1)
        #expect(PlaybackPlan.playedAudio(f, index: 99)?.index == 1)
        var picked = f
        picked.selectedAudio = 3
        #expect(PlaybackPlan.detailsAudio(picked)?.index == 3)
        // A picked track other than the details' goes to the server even for a file as it is.
        let request = PlaybackPlan.request(itemId: 7, .init(file: f, startMs: 0, audio: f.streams(.audio)[1]),
                                           clientId: "a", caps: nil)
        #expect(request.audioIndex == 2)
        #expect(PlaybackPlan.request(itemId: 7, .init(file: f, startMs: 0, audio: f.streams(.audio)[0]),
                                     clientId: "a", caps: nil).audioIndex == nil)
    }
}
