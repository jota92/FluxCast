import Foundation
import XCTest
@testable import FluxCast

final class VectorTests: XCTestCase {
    func testCanonicalVectors() throws {
        let source = URL(fileURLWithPath: #filePath)
        let vectorsURL = source
            .deletingLastPathComponent()
            .appendingPathComponent("../../../../spec/test-vectors.json")
            .standardizedFileURL
        let data = try Data(contentsOf: vectorsURL)
        let vectors = try JSONDecoder().decode(VectorFile.self, from: data)
        for vector in vectors.vectors {
            let payload = try bytes(hex: vector.payloadHex)
            var header = FCDPHeader()
            header.packetType = vector.header.packetType; header.flags = vector.header.flags
            header.sessionID = vector.header.sessionID; header.streamID = vector.header.streamID
            header.epoch = vector.header.epoch; header.sequence = vector.header.sequenceNumber
            header.frameID = vector.header.frameID; header.fragmentIndex = vector.header.fragmentIndex
            header.fragmentCount = vector.header.fragmentCount; header.priority = vector.header.priority
            header.deadlineMS = vector.header.deadlineMS
            let packet = try FCDP.encode(header: header, payload: payload)
            XCTAssertEqual(hex(packet), vector.packetHex, vector.name)
            let (decoded, body) = try FCDP.decode(packet)
            XCTAssertEqual(decoded, header, vector.name)
            XCTAssertEqual(body, payload, vector.name)
        }
    }
}

private struct VectorFile: Decodable { let vectors: [Vector] }
private struct Vector: Decodable { let name: String; let header: VectorHeader; let payloadHex: String; let packetHex: String
    enum CodingKeys: String, CodingKey { case name, header; case payloadHex = "payload_hex"; case packetHex = "packet_hex" }
}
private struct VectorHeader: Decodable {
    let packetType: UInt8; let flags: UInt8; let sessionID: UInt64; let streamID: UInt16; let epoch: UInt16
    let sequenceNumber: UInt32; let frameID: UInt32; let fragmentIndex: UInt16; let fragmentCount: UInt16
    let priority: UInt8; let deadlineMS: UInt16
    enum CodingKeys: String, CodingKey {
        case packetType = "packet_type", flags, sessionID = "session_id", streamID = "stream_id", epoch
        case sequenceNumber = "sequence_number", frameID = "frame_id", fragmentIndex = "fragment_index"
        case fragmentCount = "fragment_count", priority, deadlineMS = "deadline_ms"
    }
}
private func bytes(hex: String) throws -> [UInt8] {
    guard hex.count.isMultiple(of: 2) else { throw CocoaError(.fileReadCorruptFile) }
    return stride(from: 0, to: hex.count, by: 2).map { index in
        UInt8(hex[hex.index(hex.startIndex, offsetBy: index)..<hex.index(hex.startIndex, offsetBy: index + 2)], radix: 16)!
    }
}
private func hex(_ bytes: [UInt8]) -> String { bytes.map { String(format: "%02x", $0) }.joined() }
