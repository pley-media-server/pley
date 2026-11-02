import PleyModel
@testable import PleyPlayer
import Testing

private func tracks(_ count: Int) -> [ItemSummary] {
    (1...count).map { ItemSummary(id: $0, libraryId: 3, kind: .track, title: "Track \($0)") }
}

/// Picks the last place each time: Fisher–Yates then reverses what follows the first.
private let last: @Sendable (Int) -> Int = { $0 - 1 }

struct MusicQueueTests {
    @Test func playsInOrderFromTheStart() {
        var queue = MusicQueue(random: last)
        queue.start(tracks(3), at: 1)
        #expect(queue.current?.id == 2)
        #expect(queue.upNext.map(\.id) == [3])
        let r1 = queue.next()

        #expect(r1)
        #expect(queue.current?.id == 3)
        #expect(!queue.hasNext)
        let r2 = queue.next()

        #expect(!r2)
        #expect(queue.current?.id == 3)
    }

    @Test func aStartOutsideTheTracksIsClamped() {
        var queue = MusicQueue(random: last)
        queue.start(tracks(2), at: 9)
        #expect(queue.current?.id == 2)
        queue.start([], at: 0)
        #expect(queue.isEmpty && queue.current == nil && !queue.hasNext)
    }

    @Test func shuffleKeepsTheTrackPlayingFirst() {
        var queue = MusicQueue(random: { _ in 0 })
        queue.start(tracks(5), at: 2, shuffle: true)
        #expect(queue.shuffle)
        #expect(queue.index == 0)
        #expect(queue.current?.id == 3)
        #expect(Set(queue.tracks.map(\.id)) == [1, 2, 3, 4, 5])
        #expect(queue.tracks.map(\.id) != [3, 1, 2, 4, 5])
    }

    @Test func unshuffleGoesBackToTheOrderGivenAtTheSameTrack() {
        var queue = MusicQueue(random: { _ in 0 })
        queue.start(tracks(4), at: 0, shuffle: true)
        let r3 = queue.next()

        #expect(r3)
        let playing = queue.current?.id
        queue.setShuffle(false)
        #expect(!queue.shuffle)
        #expect(queue.tracks.map(\.id) == [1, 2, 3, 4])
        #expect(queue.current?.id == playing)
    }

    @Test func previousRestartsAfterThreeSecondsOrOnTheFirstTrack() {
        var queue = MusicQueue()
        queue.start(tracks(3), at: 1)
        let r4 = queue.previous(positionMs: 3_001)

        #expect(r4 == .restart)
        #expect(queue.current?.id == 2)
        let r5 = queue.previous(positionMs: 3_000)

        #expect(r5 == .moved)
        #expect(queue.current?.id == 1)
        let r6 = queue.previous(positionMs: 0)

        #expect(r6 == .restart)
        #expect(queue.current?.id == 1)
    }

    @Test func jumpMovesWithinThePlayOrder() {
        var queue = MusicQueue()
        queue.start(tracks(3))
        let r7 = queue.jump(to: 2)

        #expect(r7)
        #expect(queue.current?.id == 3)
        let r8 = queue.jump(to: 3)

        #expect(!r8)
        #expect(queue.current?.id == 3)
    }

    @Test func clearForgetsEverything() {
        var queue = MusicQueue()
        queue.start(tracks(3), shuffle: true)
        queue.clear()
        #expect(queue.isEmpty && !queue.shuffle && queue.tracks.isEmpty)
    }
}
