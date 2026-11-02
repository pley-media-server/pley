import Foundation

/// What the sleep timer is set to. It isn't remembered between playbacks.
public enum SleepChoice: Hashable, Sendable, Identifiable {
    case off
    case minutes(Int)
    /// The end of the track, episode or film playing.
    case endOfItem

    /// The menu's choices, in order.
    public static let all: [SleepChoice] = [.off, .minutes(15), .minutes(30), .minutes(45), .minutes(60), .minutes(90),
                                            .endOfItem]

    public var id: String {
        switch self {
        case .off: "off"
        case .minutes(let n): "\(n)"
        case .endOfItem: "end"
        }
    }

    /// The menu's words: "Off", "15 minutes", "End of this track" (`item`: "track", "episode").
    public func label(item: String) -> String {
        switch self {
        case .off: "Off"
        case .minutes(let n): "\(n) minutes"
        case .endOfItem: "End of this \(item)"
        }
    }
}

/// Where the sleep timer stands.
public struct SleepState: Equatable, Sendable {
    public var choice: SleepChoice = .off
    /// Seconds before it pauses, during the notice (its last 30 s); `nil` otherwise.
    public var secondsLeft: Int?
    /// Minutes before it pauses, rounded up, for the menu; `nil` when off or unknown.
    public var minutesLeft: Int?
    /// It paused the playback, which hasn't played since.
    public var fired = false

    public init(choice: SleepChoice = .off, secondsLeft: Int? = nil, minutesLeft: Int? = nil, fired: Bool = false) {
        self.choice = choice
        self.secondsLeft = secondsLeft
        self.minutesLeft = minutesLeft
        self.fired = fired
    }

    /// The notice's words in its last 30 s: "Pausing in 30 s"; `nil` before.
    public var notice: String? { secondsLeft.map { "Pausing in \($0) s" } }

    /// What the menu says beside "Sleep Timer": "15 min left", "End of this
    /// track", "Pausing in 12 s"; `nil` when off.
    public func summary(item: String) -> String? {
        if let notice { return notice }
        switch choice {
        case .off: return nil
        case .endOfItem: return choice.label(item: item)
        case .minutes: return minutesLeft.map { "\($0) min left" }
        }
    }
}

/// The sleep timer of music or a video, a port of the Android app's
/// `SleepTimer`: after the minutes chosen, or at the end of what plays, it
/// pauses (`onFire`). In its last 30 s, `state` counts down once a second
/// for a notice; `extend` (Keep listening, Keep watching) adds the same
/// time again. Its time is the player's clock, so tests move it by hand
/// and call `tick()`.
@MainActor
public final class SleepTimer {
    /// The notice shows this long before the timer pauses.
    public static let noticeMs = 30_000
    static let secondMs = 1_000
    static let minuteMs = 60_000

    public private(set) var state = SleepState() {
        didSet { if state != oldValue { onChange?(state) } }
    }
    /// Hears every change of `state`.
    public var onChange: (@MainActor (SleepState) -> Void)?

    private let clock: any PlayerClock
    /// How long the item playing has left, in ms of playback; `nil` while unknown.
    private let itemLeftMs: @MainActor () -> Int?
    /// Tells items apart: the one playing now.
    private let itemKey: @MainActor () -> AnyHashable?
    private let onFire: @MainActor () -> Void
    private var task: Task<Void, Never>?
    /// When a timer of minutes ends, on the clock.
    private var deadline: Int64 = 0
    /// Keep listening at the end of this item: the timer waits for the next one's end.
    private var spared: AnyHashable?

    public init(clock: any PlayerClock, itemLeftMs: @escaping @MainActor () -> Int?,
                itemKey: @escaping @MainActor () -> AnyHashable?, onFire: @escaping @MainActor () -> Void) {
        self.clock = clock
        self.itemLeftMs = itemLeftMs
        self.itemKey = itemKey
        self.onFire = onFire
    }

    public func set(_ choice: SleepChoice) {
        stop()
        spared = nil
        state = SleepState(choice: choice)
        switch choice {
        case .off: return
        case .minutes(let n): deadline = clock.now() + Int64(n * Self.minuteMs)
        case .endOfItem: break
        }
        run()
    }

    /// Adds the same time again: the minutes chosen, or until the end of the next item.
    public func extend() {
        switch state.choice {
        case .off: return
        case .minutes(let n): deadline += Int64(n * Self.minuteMs)
        case .endOfItem: spared = itemKey()
        }
        state.secondsLeft = nil
        run()
    }

    /// Whether the timer ends with the item `key`: what comes next waits.
    public func endsWith(_ key: AnyHashable?) -> Bool {
        state.choice == .endOfItem && key != spared
    }

    /// The item `key` played to its end: `true` when that is the timer's
    /// end, which then fires (the caller holds what comes next).
    public func itemEnded(_ key: AnyHashable?) -> Bool {
        guard endsWith(key) else { return false }
        fire()
        return true
    }

    /// It plays again: the pause the timer made is over.
    public func resumed() {
        if state.fired { state.fired = false }
    }

    public func cancel() {
        stop()
        spared = nil
        state = SleepState()
    }

    /// Looks at the time: publishes what is left, and fires a timer of
    /// minutes that ran out. Returns how long to wait before looking again
    /// (`nil`: it is done).
    @discardableResult
    public func tick() -> Int? {
        let choice = state.choice
        let key = itemKey()
        let left: Int?
        switch choice {
        case .off: return nil
        case .minutes: left = Int(deadline - clock.now())
        case .endOfItem: left = key == spared ? nil : itemLeftMs()
        }
        if spared != nil, key != spared { spared = nil }
        switch left {
        case nil:
            publish(nil, nil)
        case let left? where left <= 0:
            if case .minutes = choice {
                fire()
                return nil
            }
            // The end of an item fires through `itemEnded`, when it truly ends.
            publish(nil, nil)
        case let left? where left <= Self.noticeMs:
            publish((left + Self.secondMs - 1) / Self.secondMs, 1)
        case let left?:
            publish(nil, (left + Self.minuteMs - 1) / Self.minuteMs)
        }
        return wait(choice, left)
    }

    // MARK: Inside

    /// Looks at once, then each time `tick` says.
    private func run() {
        stop()
        guard let first = tick() else { return }
        let clock = clock
        task = Task { [weak self] in
            var wait = first
            while !Task.isCancelled {
                do { try await clock.sleep(ms: Int64(wait)) } catch { return }
                guard !Task.isCancelled, let next = self?.tick() else { return }
                wait = next
            }
        }
    }

    private func stop() {
        task?.cancel()
        task = nil
    }

    /// Until the next second during the notice; else until the notice or
    /// the next minute, looking at an item's end each second.
    private func wait(_ choice: SleepChoice, _ left: Int?) -> Int {
        guard let left, left > 0, choice != .endOfItem else { return Self.secondMs }
        if left <= Self.noticeMs { return left % Self.secondMs > 0 ? left % Self.secondMs : Self.secondMs }
        return min(left - Self.noticeMs, left % Self.minuteMs > 0 ? left % Self.minuteMs : Self.minuteMs)
    }

    private func publish(_ secondsLeft: Int?, _ minutesLeft: Int?) {
        var next = state
        next.secondsLeft = secondsLeft
        next.minutesLeft = minutesLeft
        state = next
    }

    private func fire() {
        stop()
        spared = nil
        state = SleepState(fired: true)
        onFire()
    }
}
