import Foundation
import os
import PleyCommon
import PleyNetwork

/// What a transfer says about a piece, named by its download's key (the
/// task's `taskDescription`, never an address).
public enum TransferEvent: Sendable {
    /// `written` bytes of the piece came so far.
    case progress(key: String, written: Int64)
    /// The piece ended: its file (moved out of the system's temporary
    /// folder before the delegate returned), the answer's status and
    /// headers, or the error and its resume data; `privateRoute` is false
    /// when a plain http answer came from an address that isn't private.
    case finished(Finished)
    /// The system delivered every event it held for the app (woken in the background).
    case allDelivered
}

public struct Finished: Sendable {
    public var key: String
    public var file: URL?
    public var status: Int
    /// Names in lower case.
    public var headers: [String: String]
    /// The transfer failed (the network, the server going away); `nil` when an answer came.
    public var failure: String?
    public var resumeData: Data?
    public var privateRoute: Bool
    public var cancelled: Bool
}

/// Where pieces go: a URLSession of download tasks, in the background for
/// the app (the system runs them while the app is suspended or closed)
/// and in the foreground for the command-line check, which has no bundle.
public protocol TransferSession: AnyObject, Sendable {
    var events: AsyncStream<TransferEvent> { get }
    /// Sends `request` for the piece of `key`'s download.
    func start(_ request: URLRequest, key: String)
    /// Goes on from where a failed piece stopped.
    func resume(_ data: Data, key: String)
    func cancel(key: String)
    /// Stops `key`'s piece where it is: it ends as a failure with its
    /// resume data, to go on once the server has proved itself again.
    func suspend(key: String)
    /// The downloads with a piece in flight (after the app starts again).
    func running() async -> Set<String>
}

/// `TransferSession` over a `URLSession`: one download task per piece.
/// The delegate's methods run on the session's own queue, never the main
/// actor: each hands its event on through `events`. A finished piece's
/// file is moved before `didFinishDownloadingTo` returns, as the system
/// deletes it then. Redirects aren't followed: the token goes only to the
/// address asked.
public final class PieceTransfer: NSObject, TransferSession, URLSessionDownloadDelegate, @unchecked Sendable {
    // Unchecked Sendable: `session` is set once in init; the rest is behind the lock.

    /// The app's background session's identifier, beside its bundle's.
    public static func backgroundIdentifier(bundle: String) -> String { bundle + ".downloads" }

    public let events: AsyncStream<TransferEvent>
    private let continuation: AsyncStream<TransferEvent>.Continuation
    private let folder: URL
    private let gate: ConnectionGate
    private var session: URLSession!

    private struct PieceState {
        var file: URL?
        var privateRoute = true
        /// Stopped by `suspend`: it ends as a failure, not a cancellation.
        var suspended = false
        var resumeData: Data?
    }

    private let tasks = OSAllocatedUnfairLock<[Int: PieceState]>(initialState: [:])

    /// A session made with `configuration`; finished pieces go to `folder`.
    /// `gate` checks plain http answers came from a private address.
    public init(configuration: URLSessionConfiguration, folder: URL, gate: ConnectionGate) {
        (events, continuation) = AsyncStream.makeStream(of: TransferEvent.self)
        self.folder = folder
        self.gate = gate
        super.init()
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        configuration.urlCredentialStorage = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        session = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
    }

    /// The app's: in the background, run by the system, waking the app when pieces finish.
    public static func background(bundle: String, folder: URL, gate: ConnectionGate) -> PieceTransfer {
        let configuration = URLSessionConfiguration.background(withIdentifier: backgroundIdentifier(bundle: bundle))
        configuration.sessionSendsLaunchEvents = true
        configuration.isDiscretionary = false
        return PieceTransfer(configuration: configuration, folder: folder, gate: gate)
    }

    /// Stops the session (tests, the command-line check).
    public func invalidate() {
        session.invalidateAndCancel()
        continuation.finish()
    }

    public func start(_ request: URLRequest, key: String) {
        let task = session.downloadTask(with: request)
        task.taskDescription = key
        task.resume()
    }

    public func resume(_ data: Data, key: String) {
        let task = session.downloadTask(withResumeData: data)
        task.taskDescription = key
        task.resume()
    }

    public func cancel(key: String) {
        session.getAllTasks { tasks in
            for task in tasks where task.taskDescription == key { task.cancel() }
        }
    }

    public func suspend(key: String) {
        session.getAllTasks { [weak self] tasks in
            for case let task as URLSessionDownloadTask in tasks where task.taskDescription == key {
                let id = task.taskIdentifier
                self?.tasks.withLock { $0[id, default: PieceState()].suspended = true }
                task.cancel { [weak self] data in
                    // Kept for the end, unless it came already (then the error carried it).
                    self?.tasks.withLock { if $0[id] != nil { $0[id]?.resumeData = data } }
                }
            }
        }
    }

    public func running() async -> Set<String> {
        let all = await session.allTasks
        return Set(all.filter { $0.state == .running || $0.state == .suspended }.compactMap(\.taskDescription))
    }

    // MARK: The delegate (the session's queue)

    public func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                           didFinishDownloadingTo location: URL) {
        let fm = FileManager.default
        let kept = folder.appendingPathComponent(UUID().uuidString + ".piece")
        do {
            try fm.createDirectory(at: folder, withIntermediateDirectories: true)
            try fm.moveItem(at: location, to: kept)
            tasks.withLock { $0[downloadTask.taskIdentifier, default: PieceState()].file = kept }
        } catch {
            Log.downloads.error("a finished piece couldn't be kept")
        }
    }

    public func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
                           totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard let key = downloadTask.taskDescription else { return }
        continuation.yield(.progress(key: key, written: totalBytesWritten))
    }

    public func urlSession(_ session: URLSession, task: URLSessionTask, didFinishCollecting metrics: URLSessionTaskMetrics) {
        guard task.originalRequest?.url?.scheme?.lowercased() == "http" else { return }
        let allowed = (try? gate.verify(metrics: metrics)) != nil
        tasks.withLock { $0[task.taskIdentifier, default: PieceState()].privateRoute = allowed }
    }

    public func urlSession(_ session: URLSession, task: URLSessionTask,
                           willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
                           completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }

    public func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        let kept = tasks.withLock { $0.removeValue(forKey: task.taskIdentifier) } ?? PieceState()
        guard let key = task.taskDescription else {
            if let file = kept.file { try? FileManager.default.removeItem(at: file) }
            return
        }
        let response = task.response as? HTTPURLResponse
        var headers: [String: String] = [:]
        for (name, value) in response?.allHeaderFields ?? [:] {
            if let name = name as? String, let value = value as? String { headers[name.lowercased()] = value }
        }
        let nsError = error as NSError?
        let cancelled = nsError?.domain == NSURLErrorDomain && nsError?.code == NSURLErrorCancelled
        // Only what the system calls the transfer failing is one; an answer is judged by its status.
        var failure: String? = error == nil ? nil : "\(nsError?.domain ?? "") \(nsError?.code ?? 0)"
        if kept.suspended && cancelled { failure = "suspended" }
        let resumeData = nsError?.userInfo[NSURLSessionDownloadTaskResumeData] as? Data ?? kept.resumeData
        continuation.yield(.finished(Finished(
            key: key, file: error == nil ? kept.file : nil, status: response?.statusCode ?? 0, headers: headers,
            failure: failure, resumeData: resumeData, privateRoute: kept.privateRoute,
            cancelled: cancelled && !kept.suspended)))
        if error != nil, let file = kept.file { try? FileManager.default.removeItem(at: file) }
    }

    public func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        continuation.yield(.allDelivered)
    }
}
