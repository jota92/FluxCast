import AVFoundation
import Foundation
import Network
import VideoToolbox

final class CameraPublisher: NSObject, ObservableObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    let session = AVCaptureSession()
    @Published var status = "Enter this Mac's LAN IP address, then start."
    @Published private(set) var isPublishing = false

    private let captureQueue = DispatchQueue(label: "com.fluxcast.camera.capture")
    private let transportQueue = DispatchQueue(label: "com.fluxcast.camera.transport")
    private let output = AVCaptureVideoDataOutput()
    private var encoder: VTCompressionSession?
    private var connection: NWConnection?
    private var sequence: UInt32 = 0
    private var frameID: UInt32 = 0
    private var sessionID = UInt64.random(in: .min ... .max)

    func start(host: String, port: UInt16) {
        guard !host.isEmpty, let endpointPort = NWEndpoint.Port(rawValue: port) else {
            status = "Enter a receiver address and UDP port."
            return
        }
        AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
            DispatchQueue.main.async {
                guard granted else { self?.status = "Camera permission is required."; return }
                self?.startAuthorized(host: host, port: endpointPort)
            }
        }
    }

    private func startAuthorized(host: String, port: NWEndpoint.Port) {
        do {
            try configureCapture()
            let connection = NWConnection(host: NWEndpoint.Host(host), port: port, using: .udp)
            connection.stateUpdateHandler = { [weak self] state in
                DispatchQueue.main.async {
                    switch state {
                    case .ready: self?.status = "Sending H.264 to \(host):\(port)."
                    case .failed(let error): self?.status = "UDP failed: \(error.localizedDescription)"; self?.stop()
                    default: break
                    }
                }
            }
            connection.start(queue: transportQueue)
            self.connection = connection
            session.startRunning()
            isPublishing = true
        } catch {
            status = "Could not start camera: \(error.localizedDescription)"
            stop()
        }
    }

    func stop() {
        session.stopRunning()
        connection?.cancel()
        connection = nil
        if let encoder { VTCompressionSessionInvalidate(encoder) }
        encoder = nil
        isPublishing = false
        status = "Stopped."
    }

    private func configureCapture() throws {
        if session.isRunning { return }
        session.beginConfiguration()
        defer { session.commitConfiguration() }
        session.sessionPreset = .hd1280x720
        for input in session.inputs { session.removeInput(input) }
        guard let camera = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back) else { throw CameraError.cameraUnavailable }
        let input = try AVCaptureDeviceInput(device: camera)
        guard session.canAddInput(input) else { throw CameraError.cannotAddInput }
        session.addInput(input)
        output.alwaysDiscardsLateVideoFrames = true
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange]
        output.setSampleBufferDelegate(self, queue: captureQueue)
        guard session.canAddOutput(output) else { throw CameraError.cannotAddOutput }
        session.addOutput(output)
        try configureEncoder(width: 1280, height: 720)
    }

    private func configureEncoder(width: Int32, height: Int32) throws {
        if let encoder { VTCompressionSessionInvalidate(encoder) }
        var created: VTCompressionSession?
        let result = VTCompressionSessionCreate(allocator: nil, width: width, height: height, codecType: kCMVideoCodecType_H264, encoderSpecification: nil, imageBufferAttributes: nil, compressedDataAllocator: nil, outputCallback: compressionCallback, refcon: Unmanaged.passUnretained(self).toOpaque(), compressionSessionOut: &created)
        guard result == noErr, let created else { throw CameraError.encoderUnavailable }
        VTSessionSetProperty(created, key: kVTCompressionPropertyKey_RealTime, value: kCFBooleanTrue)
        VTSessionSetProperty(created, key: kVTCompressionPropertyKey_AllowFrameReordering, value: kCFBooleanFalse)
        VTSessionSetProperty(created, key: kVTCompressionPropertyKey_AverageBitRate, value: NSNumber(value: 1_500_000))
        VTSessionSetProperty(created, key: kVTCompressionPropertyKey_ExpectedFrameRate, value: NSNumber(value: 30))
        VTSessionSetProperty(created, key: kVTCompressionPropertyKey_MaxKeyFrameInterval, value: NSNumber(value: 60))
        VTCompressionSessionPrepareToEncodeFrames(created)
        encoder = created
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let image = CMSampleBufferGetImageBuffer(sampleBuffer), let encoder else { return }
        let forceKeyframe: CFDictionary? = frameID == 0 ? [kVTEncodeFrameOptionKey_ForceKeyFrame as String: true] as CFDictionary : nil
        VTCompressionSessionEncodeFrame(encoder, imageBuffer: image, presentationTimeStamp: CMSampleBufferGetPresentationTimeStamp(sampleBuffer), duration: .invalid, frameProperties: forceKeyframe, sourceFrameRefcon: nil, infoFlagsOut: nil)
    }

    fileprivate func didEncode(_ sample: CMSampleBuffer) {
        guard let format = CMSampleBufferGetFormatDescription(sample), let block = CMSampleBufferGetDataBuffer(sample) else { return }
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[CFString: Any]]
        let isKeyframe = !((attachments?.first?[kCMSampleAttachmentKey_NotSync] as? Bool) ?? false)
        var accessUnit = [UInt8]()
        if isKeyframe { appendParameterSets(from: format, to: &accessUnit) }
        var length = 0
        var pointer: UnsafeMutablePointer<Int8>?
        guard CMBlockBufferGetDataPointer(block, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &pointer) == kCMBlockBufferNoErr, let pointer else { return }
        let bytes = UnsafeBufferPointer(start: UnsafeRawPointer(pointer).assumingMemoryBound(to: UInt8.self), count: length)
        var offset = 0
        while offset + 4 <= bytes.count {
            let nalLength = Int(bytes[offset]) << 24 | Int(bytes[offset + 1]) << 16 | Int(bytes[offset + 2]) << 8 | Int(bytes[offset + 3])
            offset += 4
            guard nalLength >= 0, offset + nalLength <= bytes.count else { return }
            accessUnit += [0, 0, 0, 1]
            accessUnit += bytes[offset..<(offset + nalLength)]
            offset += nalLength
        }
        send(accessUnit: accessUnit, keyframe: isKeyframe)
    }

    private func appendParameterSets(from format: CMFormatDescription, to output: inout [UInt8]) {
        for index in 0..<2 {
            var pointer: UnsafePointer<UInt8>?
            var length = 0
            var count = 0
            var nalHeaderLength: Int32 = 0
            guard CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format, parameterSetIndex: index, parameterSetPointerOut: &pointer, parameterSetSizeOut: &length, parameterSetCountOut: &count, nalUnitHeaderLengthOut: &nalHeaderLength) == noErr, let pointer else { continue }
            output += [0, 0, 0, 1]
            output += UnsafeBufferPointer(start: pointer, count: length)
        }
    }

    private func send(accessUnit: [UInt8], keyframe: Bool) {
        guard let connection, !accessUnit.isEmpty else { return }
        frameID &+= 1
        let payloadLimit = FCDP.maxDatagramLength - FCDP.headerLength
        let parts = stride(from: 0, to: accessUnit.count, by: payloadLimit).map { start in Array(accessUnit[start..<min(start + payloadLimit, accessUnit.count)]) }
        guard parts.count <= Int(UInt16.max) else { return }
        for (index, payload) in parts.enumerated() {
            sequence &+= 1
            var header = FCDPHeader()
            header.sessionID = sessionID; header.streamID = 1; header.sequence = sequence; header.frameID = frameID
            header.fragmentIndex = UInt16(index); header.fragmentCount = UInt16(parts.count)
            header.priority = keyframe ? 0 : 2; header.deadlineMS = 300
            guard let packet = try? FCDP.encode(header: header, payload: payload) else { continue }
            connection.send(content: Data(packet), completion: .contentProcessed { _ in })
        }
    }
}

private func compressionCallback(outputCallbackRefCon: UnsafeMutableRawPointer?, sourceFrameRefCon: UnsafeMutableRawPointer?, status: OSStatus, infoFlags: VTEncodeInfoFlags, sampleBuffer: CMSampleBuffer?) {
    guard status == noErr, let sampleBuffer, let outputCallbackRefCon else { return }
    Unmanaged<CameraPublisher>.fromOpaque(outputCallbackRefCon).takeUnretainedValue().didEncode(sampleBuffer)
}

private enum CameraError: LocalizedError {
    case cameraUnavailable, cannotAddInput, cannotAddOutput, encoderUnavailable
    var errorDescription: String? {
        switch self {
        case .cameraUnavailable: return "No back camera is available."
        case .cannotAddInput: return "The camera input could not be configured."
        case .cannotAddOutput: return "The camera output could not be configured."
        case .encoderUnavailable: return "The H.264 encoder is unavailable."
        }
    }
}
