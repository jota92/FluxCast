//! Canonical FCDP v0.1 wire test vectors.
//!
//! `spec/test-vectors.json` is the cross-language source of truth: every SDK
//! (Rust, Python, Node.js, Go, Swift, Kotlin, and the C example) must encode
//! these exact bytes and decode them back to the same fields. This test proves
//! the Rust reference stays byte-identical to the committed file.
//!
//! Regenerate the file after an intentional wire change with:
//!
//! ```sh
//! FLUXCAST_BLESS=1 cargo test -p fluxcast-proto --test vectors
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;

use fluxcast_proto::{Header, PacketType};

struct Vector {
    name: &'static str,
    header: Header,
    payload: Vec<u8>,
}

fn vectors() -> Vec<Vector> {
    let mut media_minimal = Header::new(PacketType::Media);
    media_minimal.session_id = 42;
    media_minimal.stream_id = 1;
    media_minimal.sequence_number = 9;
    media_minimal.frame_id = 3;
    media_minimal.priority = 2;
    media_minimal.deadline_ms = 120;

    let mut ping_empty = Header::new(PacketType::Ping);
    ping_empty.session_id = 1;

    let mut media_fragment = Header::new(PacketType::Media);
    // Kept below 2^53 so every language's native JSON number parser is lossless.
    media_fragment.session_id = 0x0001_0203_0405_0607;
    media_fragment.stream_id = 0x1112;
    media_fragment.epoch = 0x2122;
    media_fragment.sequence_number = 0x3132_3334;
    media_fragment.frame_id = 0x4142_4344;
    media_fragment.fragment_index = 2;
    media_fragment.fragment_count = 5;
    media_fragment.priority = 1;
    media_fragment.deadline_ms = 1000;

    let mut keyframe_request = Header::new(PacketType::KeyframeRequest);
    keyframe_request.session_id = 7;

    vec![
        Vector {
            name: "media_minimal",
            header: media_minimal,
            payload: b"opus-or-h264".to_vec(),
        },
        Vector {
            name: "ping_empty",
            header: ping_empty,
            payload: Vec::new(),
        },
        Vector {
            name: "media_fragment",
            header: media_fragment,
            payload: vec![0xAA; 16],
        },
        Vector {
            name: "keyframe_request",
            header: keyframe_request,
            payload: Vec::new(),
        },
    ]
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("write to String is infallible");
    }
    output
}

/// Renders the vectors to the exact JSON text committed in the spec directory.
fn render_json() -> String {
    let mut out = String::new();
    let write = |out: &mut String, line: &str| out.push_str(line);
    write(&mut out, "{\n");
    write(&mut out, "  \"format\": \"fcdp-v0.1\",\n");
    write(&mut out, "  \"header_len\": 37,\n");
    write(&mut out, "  \"vectors\": [\n");
    let vectors = vectors();
    for (index, vector) in vectors.iter().enumerate() {
        let mut packet = Vec::new();
        vector
            .header
            .encode(&vector.payload, &mut packet)
            .expect("vector encodes");
        let h = vector.header;
        write(&mut out, "    {\n");
        writeln!(out, "      \"name\": \"{}\",", vector.name).unwrap();
        write(&mut out, "      \"header\": {\n");
        writeln!(out, "        \"version\": {},", h.version).unwrap();
        writeln!(out, "        \"packet_type\": {},", h.packet_type as u8).unwrap();
        writeln!(out, "        \"flags\": {},", h.flags).unwrap();
        writeln!(out, "        \"session_id\": {},", h.session_id).unwrap();
        writeln!(out, "        \"stream_id\": {},", h.stream_id).unwrap();
        writeln!(out, "        \"epoch\": {},", h.epoch).unwrap();
        writeln!(out, "        \"sequence_number\": {},", h.sequence_number).unwrap();
        writeln!(out, "        \"frame_id\": {},", h.frame_id).unwrap();
        writeln!(out, "        \"fragment_index\": {},", h.fragment_index).unwrap();
        writeln!(out, "        \"fragment_count\": {},", h.fragment_count).unwrap();
        writeln!(out, "        \"priority\": {},", h.priority).unwrap();
        writeln!(out, "        \"deadline_ms\": {}", h.deadline_ms).unwrap();
        write(&mut out, "      },\n");
        writeln!(out, "      \"payload_hex\": \"{}\",", hex(&vector.payload)).unwrap();
        writeln!(out, "      \"packet_hex\": \"{}\"", hex(&packet)).unwrap();
        write(
            &mut out,
            if index + 1 == vectors.len() {
                "    }\n"
            } else {
                "    },\n"
            },
        );
    }
    write(&mut out, "  ]\n");
    write(&mut out, "}\n");
    out
}

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("spec")
        .join("test-vectors.json")
}

#[test]
fn rust_encoding_matches_committed_vectors() {
    let rendered = render_json();
    let path = vectors_path();

    if std::env::var_os("FLUXCAST_BLESS").is_some() {
        std::fs::write(&path, &rendered).expect("write vectors");
        return;
    }

    // Git may check the JSON out as CRLF on Windows; preserve the canonical
    // logical vector content while accepting that platform text conversion.
    let committed = std::fs::read_to_string(&path)
        .expect("read spec/test-vectors.json")
        .replace("\r\n", "\n");
    assert_eq!(
        committed, rendered,
        "spec/test-vectors.json is stale; regenerate with FLUXCAST_BLESS=1"
    );
}

#[test]
fn every_vector_round_trips_through_the_decoder() {
    for vector in vectors() {
        let mut packet = Vec::new();
        vector.header.encode(&vector.payload, &mut packet).unwrap();
        let (mut decoded, payload) = Header::decode(&packet).unwrap();
        // `payload_len` is populated only on decode; normalize before comparing.
        decoded.payload_len = vector.header.payload_len;
        assert_eq!(decoded, vector.header, "{} header", vector.name);
        assert_eq!(
            payload,
            vector.payload.as_slice(),
            "{} payload",
            vector.name
        );
    }
}
