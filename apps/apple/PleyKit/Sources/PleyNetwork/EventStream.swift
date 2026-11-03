import Foundation
import os
import PleyCommon
import PleyModel

/// What the event stream says: a connection made (the server replays
/// nothing, so what was read before it may be stale), or an event.
public enum EventSignal: Hashable, Sendable {
    case connected
    case event(ServerEvent)
}

/// The server's events (`GET /api/events`, server-sent events), kept
/// connected while they are read:
///
/// - Each `data:` field is one `ServerEvent`; types this app doesn't know,
///   and data that can't be read, are skipped.
/// - A line longer than 64 KiB drops the connection: no event is that long.
/// - The stream connects again when it ends or fails: after 1, 2 and 5
///   seconds, then every 15; a connection that opened starts the count
///   again. A `429` (too many streams for the token) waits 30 seconds.
/// - It stops for good when the sign-in ends (`signedOut`) or the app
///   isn't allowed the stream (`403`): connecting again can't help.
///
/// It goes through the app's API client, so the token is the open
/// server's, refreshed as for every request, and plain http follows
/// Pley's rule.
public struct EventStream: Sendable {
    /// The longest line read.
    public static let maxLine = 64 << 10
    /// The waits before connecting again, in seconds; the last repeats.
    public static let backoff: [Double] = [1, 2, 5, 15]
    /// The wait after a `429`, in seconds.
    public static let tooManyWait: Double = 30

    public let api: PleyAPI
    private let sleep: @Sendable (Double) async throws -> Void

    /// `sleep` waits that many seconds (the tests wait less).
    public init(api: PleyAPI,
                sleep: @escaping @Sendable (Double) async throws -> Void = { try await Task.sleep(for: .seconds($0)) }) {
        self.api = api
        self.sleep = sleep
    }

    /// The signals, until the stream that reads them is cancelled, or the
    /// sign-in ends.
    public func signals() -> AsyncStream<EventSignal> {
        let api = api
        let sleep = sleep
        return AsyncStream { continuation in
            let task = Task {
                var failures = 0
                while !Task.isCancelled {
                    let parser = EventParser { continuation.yield(.event($0)) }
                    let opened = OSAllocatedUnfairLock(initialState: false)
                    var wait: Double
                    do {
                        try await api.client.stream("/api/events", headers: ["Accept": "text/event-stream",
                                                                            "Cache-Control": "no-cache"],
                                                    opened: { status in
                                                        guard (200..<300).contains(status) else { return }
                                                        opened.withLock { $0 = true }
                                                        continuation.yield(.connected)
                                                    },
                                                    read: parser.feed)
                        if parser.overflowed { Log.network.info("events: a line too long, connecting again") }
                        wait = 0
                    } catch is CancellationError {
                        break
                    } catch PleyError.signedOut {
                        break
                    } catch PleyError.insufficientScope {
                        break
                    } catch let error as PleyError where Self.status(error) == 403 {
                        break
                    } catch let error as PleyError where Self.status(error) == 429 {
                        wait = Self.tooManyWait
                    } catch {
                        if Task.isCancelled { break }
                        wait = 0
                    }
                    if opened.withLock({ $0 }) { failures = 0 }
                    if wait == 0 {
                        wait = Self.backoff[min(failures, Self.backoff.count - 1)]
                        failures += 1
                    }
                    do { try await sleep(wait) } catch { break }
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    private static func status(_ error: PleyError) -> Int? {
        switch error {
        case .api(let status, _, _): status
        case .unreadable(let status): status
        default: nil
        }
    }
}

/// Server-sent events read from pieces of the body as they come: lines
/// split on LF (a CR before it dropped), `data:` fields gathered until a
/// blank line ends the event, comments (`:`) and other fields skipped.
public final class EventParser: @unchecked Sendable {
    // Unchecked Sendable: every mutable thing is behind the lock.
    private struct State {
        var line = Data()
        var data: [Data] = []
        var overflowed = false
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let event: @Sendable (ServerEvent) -> Void
    private let maxLine: Int
    /// An event's `data:` lines at most: Pley's events have one.
    static let maxEventLines = 256

    public init(maxLine: Int = EventStream.maxLine, event: @escaping @Sendable (ServerEvent) -> Void) {
        self.maxLine = maxLine
        self.event = event
    }

    /// Whether a line went past the bound, which ended the reading.
    public var overflowed: Bool { state.withLock { $0.overflowed } }

    /// Reads `piece`; false (stop reading) once a line is too long.
    @discardableResult
    public func feed(_ piece: Data) -> Bool {
        let (ok, found) = state.withLock { s -> (Bool, [ServerEvent]) in
            var found: [ServerEvent] = []
            if s.overflowed { return (false, found) }
            var rest = piece[...]
            while let newline = rest.firstIndex(of: UInt8(ascii: "\n")) {
                s.line.append(contentsOf: rest[..<newline])
                rest = rest[rest.index(after: newline)...]
                if s.line.count > maxLine {
                    s.overflowed = true
                    return (false, found)
                }
                if s.line.last == UInt8(ascii: "\r") { s.line.removeLast() }
                if let e = Self.take(line: s.line, data: &s.data) { found.append(e) }
                s.line.removeAll(keepingCapacity: true)
            }
            s.line.append(contentsOf: rest)
            if s.line.count > maxLine {
                s.overflowed = true
                return (false, found)
            }
            return (true, found)
        }
        for e in found { event(e) }
        return ok
    }

    /// One line: a field kept, or the event ended by a blank line.
    private static func take(line: Data, data: inout [Data]) -> ServerEvent? {
        if line.isEmpty {
            defer { data.removeAll() }
            guard !data.isEmpty else { return nil }
            var joined = Data()
            for (n, part) in data.enumerated() {
                if n > 0 { joined.append(UInt8(ascii: "\n")) }
                joined.append(part)
            }
            guard let e = ServerEvent.parse(joined) else { return nil }
            if case .unknown = e { return nil }
            return e
        }
        if line.first == UInt8(ascii: ":") { return nil }
        let colon = line.firstIndex(of: UInt8(ascii: ":")) ?? line.endIndex
        guard line[..<colon].elementsEqual("data".utf8) else { return nil }
        var value = colon < line.endIndex ? line[line.index(after: colon)...] : Data()[...]
        if value.first == UInt8(ascii: " ") { value = value.dropFirst() }
        // An event is a few hundred bytes: one that never ends isn't kept.
        guard data.count < Self.maxEventLines else { return nil }
        data.append(Data(value))
        return nil
    }
}
