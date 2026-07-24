package fluxcast

import java.nio.file.Files
import java.nio.file.Paths

private fun number(objectText: String, key: String): Long = Regex("\\\"$key\\\"\\s*:\\s*(\\d+)")
    .find(objectText)?.groupValues?.get(1)?.toLong() ?: error("missing $key")
private fun hex(value: ByteArray): String = value.joinToString("") { "%02x".format(it.toInt() and 255) }
private fun bytes(value: String): ByteArray = value.chunked(2).map { it.toInt(16).toByte() }.toByteArray()

fun main(args: Array<String>) {
    require(args.size == 1) { "usage: VectorTest <test-vectors.json>" }
    val source = Files.readString(Paths.get(args[0]))
    val vectors = Regex(
        """\{\s*"name"\s*:\s*"([^"]+)"\s*,\s*"header"\s*:\s*\{(.*?)\}\s*,\s*"payload_hex"\s*:\s*"([^"]*)"\s*,\s*"packet_hex"\s*:\s*"([^"]+)"\s*\}""",
        setOf(RegexOption.DOT_MATCHES_ALL)
    ).findAll(source).toList()
    require(vectors.isNotEmpty()) { "no vectors found" }
    for (vector in vectors) {
        val name = vector.groupValues[1]
        val fields = vector.groupValues[2]
        val header = FcdpHeader(
            packetType = number(fields, "packet_type").toInt(), flags = number(fields, "flags").toInt(),
            sessionId = number(fields, "session_id"), streamId = number(fields, "stream_id").toInt(),
            epoch = number(fields, "epoch").toInt(), sequence = number(fields, "sequence_number"),
            frameId = number(fields, "frame_id"), fragmentIndex = number(fields, "fragment_index").toInt(),
            fragmentCount = number(fields, "fragment_count").toInt(), priority = number(fields, "priority").toInt(),
            deadlineMs = number(fields, "deadline_ms").toInt()
        )
        val payload = bytes(vector.groupValues[3])
        val packet = FluxCastFcdp.encode(header, payload)
        check(hex(packet) == vector.groupValues[4]) { "$name encode mismatch" }
        val (decoded, body) = FluxCastFcdp.decode(packet)
        check(decoded == header && body.contentEquals(payload)) { "$name decode mismatch" }
    }
    println("  ${vectors.size} vectors matched")
}
