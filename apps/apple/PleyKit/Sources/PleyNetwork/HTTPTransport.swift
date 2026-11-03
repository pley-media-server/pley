import Foundation
import os
import PleyCommon

/// How much of an answer the app reads; past it the request is cancelled
/// and fails with `.tooLarge`.
public enum BodyLimit {
    /// `/api/server`, the metadata and the sign-in endpoints' answers.
    public static let small = 64 << 10
    /// The API's JSON.
    public static let json = 8 << 20
    /// Pictures.
    public static let image = 10 << 20
}

/// An answer, read in full.
public struct HTTPResponse: Sendable {
    /// Where it came from, after any redirect followed.
    public let url: URL
    public let status: Int
    /// Header names in lower case.
    public let headers: [String: String]
    public let body: Data

    public func header(_ name: String) -> String? { headers[name.lowercased()] }

    /// `image/png` from `image/png; charset=…`, in lower case.
    public var contentType: String? {
        header("content-type")?.split(separator: ";").first?.trimmingCharacters(in: .whitespaces).lowercased()
    }
}

/// The app's one way onto the network: ephemeral URLSessions without
/// cookies or a cache, the app's User-Agent on every request, the
/// system's certificate authorities only, the redirect policy, Pley's
/// plain http rule (`ConnectionGate`) and a limit on every answer.
///
/// Plain http has a session of its own that uses no proxy: through the
/// system's HTTP proxy or PAC file, a request to a private address would
/// carry its token to the proxy, which may be anywhere, before the address
/// the connection reached could be checked. https uses the system's proxy
/// settings, as other apps do.
public final class HTTPTransport: Sendable {
    public let userAgent: String
    public let gate: ConnectionGate
    fileprivate let session: URLSession
    fileprivate let directSession: URLSession

    /// `userAgent` is `UserAgent.current`, read on the main actor.
    public init(userAgent: String, gate: ConnectionGate = ConnectionGate()) {
        self.userAgent = userAgent
        self.gate = gate
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.httpCookieAcceptPolicy = .never
        configuration.urlCache = nil
        configuration.urlCredentialStorage = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.httpAdditionalHeaders = ["User-Agent": userAgent, "Accept": "application/json"]
        configuration.timeoutIntervalForRequest = 20
        configuration.waitsForConnectivity = false
        session = URLSession(configuration: configuration)
        // An empty proxy dictionary, unlike the default nil, means no proxy at all.
        let direct = configuration.copy() as! URLSessionConfiguration
        direct.connectionProxyDictionary = [:]
        directSession = URLSession(configuration: direct)
    }

    deinit {
        session.invalidateAndCancel()
        directSession.invalidateAndCancel()
    }

    /// The proxy settings plain http requests go with: none.
    package var plainHTTPProxies: [AnyHashable: Any]? { directSession.configuration.connectionProxyDictionary }

    /// Sends `request` and reads its answer, of at most `maxBytes`.
    ///
    /// With `gated`, plain http follows Pley's rule: refused before sending
    /// when the host's addresses aren't private, and thrown away after when
    /// the connection's wasn't; `onLeak` is called then (the request
    /// carried a token, and the sign-in ends on the device). Only probes of
    /// `/api/server`, which carry nothing secret, go ungated.
    package func perform(_ request: URLRequest, redirects: RedirectPolicy.Mode, maxBytes: Int, gated: Bool = true,
                         onLeak: (@Sendable () async -> Void)? = nil) async throws -> HTTPResponse {
        guard let url = request.url, let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = url.host(percentEncoded: false), !host.isEmpty
        else { throw PleyError.unreachable("that isn't a server address") }
        let plainHTTP = scheme == "http"
        if gated && plainHTTP { try await gate.check(host: host) }
        try Task.checkCancellation()

        var request = request
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        request.httpShouldHandleCookies = false
        let method = request.httpMethod ?? "GET"
        let path = Log.path(url.path(percentEncoded: true))

        let observer = TaskObserver(redirects: redirects, maxBytes: maxBytes)
        let answer: TaskObserver.Answer
        do {
            answer = try await observer.run(plainHTTP ? directSession : session, request)
        } catch {
            let mapped = PleyError.from(error)
            Log.network.info("\(method, privacy: .public) \(path, privacy: .public) failed")
            throw mapped
        }
        if gated && plainHTTP {
            do {
                try gate.verify(metrics: answer.metrics)
            } catch {
                await onLeak?()
                throw error
            }
        }
        let status = answer.response.statusCode
        Log.network.debug("\(method, privacy: .public) \(path, privacy: .public) \(status, privacy: .public)")
        var headers: [String: String] = [:]
        for (name, value) in answer.response.allHeaderFields {
            if let name = name as? String, let value = value as? String { headers[name.lowercased()] = value }
        }
        return HTTPResponse(url: answer.response.url ?? url, status: status, headers: headers, body: answer.body)
    }
}

/// An answer read as it arrived (`HTTPTransport.stream`): its status and
/// headers, and, for an error status, its body (64 KiB at most).
public struct StreamedResponse: Sendable {
    public let url: URL
    public let status: Int
    public let headers: [String: String]
    public let errorBody: Data

    public func header(_ name: String) -> String? { headers[name.lowercased()] }
}

extension HTTPTransport {
    /// Sends `request` and hands each piece of a 2xx answer's body to
    /// `read` as it arrives (its byte count), until `read` returns false or
    /// the body ends; nothing is kept. An error status's body is read
    /// whole instead (64 KiB at most), for its error.
    ///
    /// Plain http follows Pley's rule, as `perform` does: refused before
    /// sending when the host's addresses aren't private, and thrown away
    /// after when the connection's wasn't (then `onLeak`).
    package func stream(_ request: URLRequest, redirects: RedirectPolicy.Mode,
                        onLeak: (@Sendable () async -> Void)? = nil,
                        read: @escaping @Sendable (Int) -> Bool) async throws -> StreamedResponse {
        try await streamData(request, redirects: redirects, onLeak: onLeak) { read($0.count) }
    }

    /// `stream`, handing `read` the pieces themselves (a download written to a file).
    /// `opened` is told the answer's status as soon as it comes, before any of its body.
    package func streamData(_ request: URLRequest, redirects: RedirectPolicy.Mode,
                            onLeak: (@Sendable () async -> Void)? = nil,
                            opened: (@Sendable (Int) -> Void)? = nil,
                            read: @escaping @Sendable (Data) -> Bool) async throws -> StreamedResponse {
        guard let url = request.url, let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = url.host(percentEncoded: false), !host.isEmpty
        else { throw PleyError.unreachable("that isn't a server address") }
        let plainHTTP = scheme == "http"
        if plainHTTP { try await gate.check(host: host) }
        try Task.checkCancellation()

        var request = request
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        request.httpShouldHandleCookies = false
        let path = Log.path(url.path(percentEncoded: true))
        let observer = StreamObserver(redirects: redirects, opened: opened, read: read)
        let answer: StreamObserver.Answer
        do {
            answer = try await observer.run(plainHTTP ? directSession : session, request)
        } catch {
            Log.network.info("GET \(path, privacy: .public) failed")
            throw PleyError.from(error)
        }
        if plainHTTP {
            do {
                try gate.verify(metrics: answer.metrics)
            } catch {
                await onLeak?()
                throw error
            }
        }
        var headers: [String: String] = [:]
        for (name, value) in answer.response.allHeaderFields {
            if let name = name as? String, let value = value as? String { headers[name.lowercased()] = value }
        }
        return StreamedResponse(url: answer.response.url ?? url, status: answer.response.statusCode, headers: headers,
                                errorBody: answer.errorBody)
    }
}

/// `streamData`'s delegate: hands a 2xx body's pieces to `read`, and stops
/// the task when it says so; keeps an error body and the metrics.
private final class StreamObserver: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    struct Answer {
        let response: HTTPURLResponse
        let errorBody: Data
        let metrics: URLSessionTaskMetrics?
    }

    private struct State {
        var continuation: CheckedContinuation<Answer, any Error>?
        var response: HTTPURLResponse?
        var errorBody = Data()
        var metrics: URLSessionTaskMetrics?
        var stopped = false
    }

    private static let maxErrorBody = 64 << 10
    private let redirects: RedirectPolicy.Mode
    private let read: @Sendable (Data) -> Bool
    private let opened: (@Sendable (Int) -> Void)?
    // Unchecked Sendable: every mutable thing is behind this lock.
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(redirects: RedirectPolicy.Mode, opened: (@Sendable (Int) -> Void)? = nil,
         read: @escaping @Sendable (Data) -> Bool) {
        self.redirects = redirects
        self.opened = opened
        self.read = read
    }

    func run(_ session: URLSession, _ request: URLRequest) async throws -> Answer {
        let task = session.dataTask(with: request)
        task.delegate = self
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                state.withLock { $0.continuation = continuation }
                task.resume()
            }
        } onCancel: {
            task.cancel()
        }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                    completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void) {
        let http = response as? HTTPURLResponse
        state.withLock { $0.response = http }
        if let http { opened?(http.statusCode) }
        completionHandler(.allow)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        let (status, stopped) = state.withLock { ($0.response?.statusCode ?? 0, $0.stopped) }
        if stopped { return }
        if !(200..<300).contains(status) {
            let over = state.withLock { s -> Bool in
                s.errorBody.append(data.prefix(Self.maxErrorBody - s.errorBody.count))
                return s.errorBody.count >= Self.maxErrorBody
            }
            if over { stop(dataTask) }
            return
        }
        if !read(data) { stop(dataTask) }
    }

    /// Ends the task early: what came is the answer.
    private func stop(_ task: URLSessionDataTask) {
        state.withLock { $0.stopped = true }
        task.cancel()
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest,
                    completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        guard let from = response.url ?? task.currentRequest?.url, let to = request.url,
              RedirectPolicy.decide(from: from, to: to, mode: redirects)
        else {
            completionHandler(nil)
            return
        }
        var next = request
        if let authorization = task.originalRequest?.value(forHTTPHeaderField: "Authorization") {
            next.setValue(authorization, forHTTPHeaderField: "Authorization")
        }
        completionHandler(next)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping @Sendable (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        completionHandler(.performDefaultHandling, nil)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didFinishCollecting metrics: URLSessionTaskMetrics) {
        state.withLock { $0.metrics = metrics }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        let (continuation, result) = state.withLock { s -> (CheckedContinuation<Answer, any Error>?, Result<Answer, any Error>) in
            let continuation = s.continuation
            s.continuation = nil
            guard let response = s.response else {
                return (continuation, .failure(error ?? PleyError.unreachable(PleyError.connectionFailed)))
            }
            // Stopped by `read` or the error body's limit: what came is the answer.
            if let error, !s.stopped { return (continuation, .failure(error)) }
            return (continuation, .success(Answer(response: response, errorBody: s.errorBody, metrics: s.metrics)))
        }
        continuation?.resume(with: result)
    }
}

/// One task's delegate: reads the answer up to its limit, decides its
/// redirects, leaves certificates to the system and keeps its metrics.
private final class TaskObserver: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    struct Answer {
        let response: HTTPURLResponse
        let body: Data
        let metrics: URLSessionTaskMetrics?
    }

    private struct State {
        var continuation: CheckedContinuation<Answer, any Error>?
        var response: HTTPURLResponse?
        var body = Data()
        var metrics: URLSessionTaskMetrics?
        var tooLarge = false
    }

    private let redirects: RedirectPolicy.Mode
    private let maxBytes: Int
    // Unchecked Sendable: every mutable thing is behind this lock.
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(redirects: RedirectPolicy.Mode, maxBytes: Int) {
        self.redirects = redirects
        self.maxBytes = maxBytes
    }

    func run(_ session: URLSession, _ request: URLRequest) async throws -> Answer {
        let task = session.dataTask(with: request)
        task.delegate = self
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                state.withLock { $0.continuation = continuation }
                task.resume()
            }
        } onCancel: {
            task.cancel()
        }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                    completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void) {
        let tooLarge = response.expectedContentLength > Int64(maxBytes)
        state.withLock {
            $0.response = response as? HTTPURLResponse
            if tooLarge { $0.tooLarge = true }
        }
        completionHandler(tooLarge ? .cancel : .allow)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        let over = state.withLock { s -> Bool in
            if s.tooLarge { return true }
            if s.body.count + data.count > maxBytes {
                s.tooLarge = true
                s.body = Data()
                return true
            }
            s.body.append(data)
            return false
        }
        if over { dataTask.cancel() }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest,
                    completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        guard let from = response.url ?? task.currentRequest?.url, let to = request.url,
              RedirectPolicy.decide(from: from, to: to, mode: redirects)
        else {
            // Not followed: the 3xx is the answer.
            completionHandler(nil)
            return
        }
        // The same origin: the request goes on as it was asked, token included.
        var next = request
        if let authorization = task.originalRequest?.value(forHTTPHeaderField: "Authorization") {
            next.setValue(authorization, forHTTPHeaderField: "Authorization")
        }
        completionHandler(next)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping @Sendable (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        // The system's trust store decides; no certificate is ever accepted by hand.
        completionHandler(.performDefaultHandling, nil)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didFinishCollecting metrics: URLSessionTaskMetrics) {
        state.withLock { $0.metrics = metrics }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        let (continuation, result) = state.withLock { s -> (CheckedContinuation<Answer, any Error>?, Result<Answer, any Error>) in
            let continuation = s.continuation
            s.continuation = nil
            if s.tooLarge { return (continuation, .failure(PleyError.tooLarge)) }
            if let error { return (continuation, .failure(error)) }
            guard let response = s.response else {
                return (continuation, .failure(PleyError.unreachable(PleyError.connectionFailed)))
            }
            return (continuation, .success(Answer(response: response, body: s.body, metrics: s.metrics)))
        }
        continuation?.resume(with: result)
    }
}
