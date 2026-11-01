import AVFoundation
import CoreVideo
import Foundation

/// Makes small H.264 MP4 files at test time with AVAssetWriter, so the
/// player's tests play real media without a file kept in the repository.
public enum MediaMaker {
    public enum MakeError: Error {
        case notWritten(String)
    }

    /// An MP4 of `seconds` of H.264 video, `width`×`height` at `fps`, each
    /// second a colour of its own, with its index at the start
    /// (`+faststart`), as bytes.
    public static func mp4(seconds: Int = 4, width: Int = 320, height: Int = 180, fps: Int = 30) async throws -> Data {
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("pley-media-\(UUID().uuidString).mp4")
        defer { try? FileManager.default.removeItem(at: file) }
        let writer = try AVAssetWriter(outputURL: file, fileType: .mp4)
        writer.shouldOptimizeForNetworkUse = true
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
        ])
        input.expectsMediaDataInRealTime = false
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: width,
            kCVPixelBufferHeightKey as String: height,
        ])
        guard writer.canAdd(input) else { throw MakeError.notWritten("the writer takes no H.264 input") }
        writer.add(input)
        guard writer.startWriting() else { throw MakeError.notWritten(String(describing: writer.error)) }
        writer.startSession(atSourceTime: .zero)
        let colours: [(UInt8, UInt8, UInt8)] = [(200, 40, 40), (40, 200, 40), (40, 40, 200), (200, 200, 40)]
        for frame in 0..<(seconds * fps) {
            while !input.isReadyForMoreMediaData { try await Task.sleep(for: .milliseconds(5)) }
            guard let pool = adaptor.pixelBufferPool else { throw MakeError.notWritten("no pixel buffer pool") }
            var made: CVPixelBuffer?
            CVPixelBufferPoolCreatePixelBuffer(nil, pool, &made)
            guard let buffer = made else { throw MakeError.notWritten("no pixel buffer") }
            fill(buffer, colours[(frame / fps) % colours.count], shade: UInt8(frame % fps * 4))
            let time = CMTime(value: CMTimeValue(frame), timescale: CMTimeScale(fps))
            guard adaptor.append(buffer, withPresentationTime: time) else {
                throw MakeError.notWritten(String(describing: writer.error))
            }
        }
        input.markAsFinished()
        await writer.finishWriting()
        guard writer.status == .completed else { throw MakeError.notWritten(String(describing: writer.error)) }
        return try Data(contentsOf: file)
    }

    private static func fill(_ buffer: CVPixelBuffer, _ colour: (UInt8, UInt8, UInt8), shade: UInt8) {
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return }
        let rowBytes = CVPixelBufferGetBytesPerRow(buffer)
        let width = CVPixelBufferGetWidth(buffer)
        for row in 0..<CVPixelBufferGetHeight(buffer) {
            let line = base.advanced(by: row * rowBytes).assumingMemoryBound(to: UInt8.self)
            for x in 0..<width {
                // BGRA, with a band that moves so frames differ.
                let band: UInt8 = (x + Int(shade)) % 64 < 8 ? 255 : 0
                line[x * 4] = colour.2 | band
                line[x * 4 + 1] = colour.1 | band
                line[x * 4 + 2] = colour.0 | band
                line[x * 4 + 3] = 255
            }
        }
    }
}
