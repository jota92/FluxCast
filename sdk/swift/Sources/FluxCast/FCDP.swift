/// Dependency-free FCDP v0.1 framing for Swift applications.
public enum FluxCastError: Error, Equatable {
    case invalidHeader
    case invalidCrc
    case invalidFragment
    case payloadLengthMismatch
    case datagramTooLarge
}

public struct FCDPHeader: Equatable {
    public var packetType: UInt8 = 3
    public var flags: UInt8 = 0
    public var sessionID: UInt64 = 1
    public var streamID: UInt16 = 1
    public var epoch: UInt16 = 0
    public var sequence: UInt32 = 1
    public var frameID: UInt32 = 1
    public var fragmentIndex: UInt16 = 0
    public var fragmentCount: UInt16 = 1
    public var priority: UInt8 = 0
    public var deadlineMS: UInt16 = 1000

    public init() {}
}

public enum FCDP {
    public static let headerLength = 37
    public static let maxDatagramLength = 1200

    public static func encode(header: FCDPHeader, payload: [UInt8]) throws -> [UInt8] {
        guard payload.count + headerLength <= maxDatagramLength else { throw FluxCastError.datagramTooLarge }
        let raw = try encodeWithoutCRC(header: header, payloadLength: payload.count)
        return raw + u16(crc16(raw)) + payload
    }

    public static func decode(_ datagram: [UInt8]) throws -> (FCDPHeader, [UInt8]) {
        guard datagram.count >= headerLength, datagram[0] == 70, datagram[1] == 67, datagram[2] == 1 else { throw FluxCastError.invalidHeader }
        let raw = Array(datagram[0..<35])
        guard crc16(raw) == read16(datagram, 35) else { throw FluxCastError.invalidCrc }
        let payloadLength = Int(read16(raw, 33))
        let payload = Array(datagram.dropFirst(headerLength))
        guard payload.count == payloadLength else { throw FluxCastError.payloadLengthMismatch }
        var header = FCDPHeader()
        header.packetType = raw[3]; header.flags = raw[4]; header.sessionID = read64(raw, 6)
        header.streamID = read16(raw, 14); header.epoch = read16(raw, 16); header.sequence = read32(raw, 18)
        header.frameID = read32(raw, 22); header.fragmentIndex = read16(raw, 26); header.fragmentCount = read16(raw, 28)
        header.priority = raw[30]; header.deadlineMS = read16(raw, 31)
        _ = try encodeWithoutCRC(header: header, payloadLength: payload.count)
        return (header, payload)
    }

    private static func encodeWithoutCRC(header: FCDPHeader, payloadLength: Int) throws -> [UInt8] {
        guard header.priority <= 3, header.fragmentCount > 0, header.fragmentIndex < header.fragmentCount, payloadLength <= Int(UInt16.max) else { throw FluxCastError.invalidFragment }
        return [70, 67, 1, header.packetType, header.flags, 0] + u64(header.sessionID) + u16(header.streamID) + u16(header.epoch) + u32(header.sequence) + u32(header.frameID) + u16(header.fragmentIndex) + u16(header.fragmentCount) + [header.priority] + u16(header.deadlineMS) + u16(UInt16(payloadLength))
    }

    private static func u16(_ value: UInt16) -> [UInt8] { [UInt8(value >> 8), UInt8(value & 255)] }
    private static func u32(_ value: UInt32) -> [UInt8] { (0..<4).reversed().map { UInt8((value >> UInt32($0 * 8)) & 255) } }
    private static func u64(_ value: UInt64) -> [UInt8] { (0..<8).reversed().map { UInt8((value >> UInt64($0 * 8)) & 255) } }
    private static func read16(_ bytes: [UInt8], _ offset: Int) -> UInt16 { UInt16(bytes[offset]) << 8 | UInt16(bytes[offset + 1]) }
    private static func read32(_ bytes: [UInt8], _ offset: Int) -> UInt32 { (0..<4).reduce(0) { $0 << 8 | UInt32(bytes[offset + $1]) } }
    private static func read64(_ bytes: [UInt8], _ offset: Int) -> UInt64 { (0..<8).reduce(0) { $0 << 8 | UInt64(bytes[offset + $1]) } }
    private static func crc16(_ bytes: [UInt8]) -> UInt16 { var value: UInt16 = 0xffff; for byte in bytes { value ^= UInt16(byte) << 8; for _ in 0..<8 { value = value & 0x8000 != 0 ? (value << 1) ^ 0x1021 : value << 1 } }; return value }
}
