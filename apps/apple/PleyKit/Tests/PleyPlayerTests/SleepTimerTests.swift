@testable import PleyPlayer
import Testing

/// The sleep timer on the fake clock: the tests call `tick()` where the
/// timer's own loop would look again.
@MainActor
private final class Sleeper {
    let clock = FakeClock()
    var left: Int?
    var key: AnyHashable? = "a"
    var fired = 0
    lazy var timer = SleepTimer(clock: clock, itemLeftMs: { [unowned self] in self.left },
                                itemKey: { [unowned self] in self.key }, onFire: { [unowned self] in self.fired += 1 })
}

@MainActor
struct SleepTimerTests {
    @Test func minutesCountDownThenFire() {
        let rig = Sleeper()
        rig.timer.set(.minutes(15))
        #expect(rig.timer.state.minutesLeft == 15)
        #expect(rig.timer.state.summary(item: "track") == "15 min left")
        rig.clock.advance(ms: 60_000 * 14 + 1_000)
        rig.timer.tick()
        #expect(rig.timer.state.minutesLeft == 1)
        #expect(rig.timer.state.notice == nil)
        rig.clock.advance(ms: 40_000)
        rig.timer.tick()
        #expect(rig.timer.state.secondsLeft == 19)
        #expect(rig.timer.state.notice == "Pausing in 19 s")
        rig.clock.advance(ms: 19_000)
        rig.timer.tick()
        #expect(rig.fired == 1)
        #expect(rig.timer.state == SleepState(fired: true))
        rig.timer.resumed()
        #expect(!rig.timer.state.fired)
    }

    @Test func keepListeningAddsTheSameTimeAgain() {
        let rig = Sleeper()
        rig.timer.set(.minutes(15))
        rig.clock.advance(ms: 60_000 * 15 - 10_000)
        rig.timer.tick()
        #expect(rig.timer.state.secondsLeft == 10)
        rig.timer.extend()
        #expect(rig.timer.state.secondsLeft == nil)
        #expect(rig.timer.state.minutesLeft == 16)
        #expect(rig.fired == 0)
    }

    @Test func theEndOfTheItemFiresWhenItEnds() {
        let rig = Sleeper()
        rig.left = 100_000
        rig.timer.set(.endOfItem)
        #expect(rig.timer.state.summary(item: "track") == "End of this track")
        #expect(rig.timer.endsWith("a"))
        rig.left = 20_000
        rig.timer.tick()
        #expect(rig.timer.state.notice == "Pausing in 20 s")
        rig.left = 0
        rig.timer.tick()
        #expect(rig.fired == 0)
        #expect(rig.timer.itemEnded("a"))
        #expect(rig.fired == 1)
        #expect(!rig.timer.itemEnded("b"))
    }

    @Test func keepListeningAtTheEndOfTheItemWaitsForTheNextOne() {
        let rig = Sleeper()
        rig.left = 10_000
        rig.timer.set(.endOfItem)
        rig.timer.extend()
        #expect(!rig.timer.itemEnded("a"))
        rig.key = "b"
        rig.timer.tick()
        #expect(rig.timer.itemEnded("b"))
        #expect(rig.fired == 1)
    }

    @Test func offAndCancelStopIt() {
        let rig = Sleeper()
        rig.timer.set(.minutes(30))
        rig.timer.set(.off)
        #expect(rig.timer.state == SleepState())
        rig.timer.set(.minutes(30))
        rig.timer.cancel()
        rig.clock.advance(ms: 60_000 * 31)
        rig.timer.tick()
        #expect(rig.fired == 0)
    }

    @Test func theMenuSaysEachChoice() {
        #expect(SleepChoice.all.map { $0.label(item: "track") } == [
            "Off", "15 minutes", "30 minutes", "45 minutes", "60 minutes", "90 minutes", "End of this track",
        ])
        #expect(SleepState().summary(item: "track") == nil)
    }
}
