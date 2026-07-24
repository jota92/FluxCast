//! Canonical wire vectors for the FCDP control payloads (FEC, NACK, ACK).
//!
//! The header vectors in `spec/test-vectors.json` pin framing; these pin the
//! payloads carried inside `FEC`, `NACK`, and `ACK` datagrams so any SDK that
//! implements recovery or feedback stays byte-compatible with the Rust core.
//!
//! Regenerate `spec/control-vectors.json` after an intentional change with:
//!
//! ```sh
//! FLUXCAST_BLESS=1 cargo test -p fluxcast-core --test control_vectors
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use fluxcast_core::{
    FecBlock, TransportFeedback, decode_fec_payload, decode_feedback_payload, decode_nack_payload,
    encode_fec_payload, encode_feedback_payload, encode_nack_payload,
};

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").unwrap();
    }
    out
}

fn sample_fec() -> FecBlock {
    // Two 4-byte source symbols "abcd" and "wxyz"; parity is their XOR.
    let a = *b"abcd";
    let b = *b"wxyz";
    let parity: Vec<u8> = a.iter().zip(b).map(|(x, y)| x ^ y).collect();
    FecBlock {
        symbol_len: 4,
        fragment_count: 2,
        original_len: 8,
        parity,
    }
}

fn render_json() -> String {
    let fec = sample_fec();
    let fec_bytes = encode_fec_payload(&fec);
    let nack = vec![1u32, 2, 4_000_000];
    let nack_bytes = encode_nack_payload(&nack);
    let feedback = TransportFeedback {
        sent: 100,
        received: 91,
        late: 3,
        rtt: Duration::from_micros(31_250),
    };
    let ack_bytes = encode_feedback_payload(feedback);

    let mut out = String::new();
    out.push_str("{\n  \"format\": \"fcdp-v0.1-control\",\n");
    writeln!(
        out,
        "  \"fec\": {{ \"symbol_len\": {}, \"fragment_count\": {}, \"original_len\": {}, \"payload_hex\": \"{}\" }},",
        fec.symbol_len,
        fec.fragment_count,
        fec.original_len,
        hex(&fec_bytes)
    )
    .unwrap();
    writeln!(
        out,
        "  \"nack\": {{ \"sequences\": [1, 2, 4000000], \"payload_hex\": \"{}\" }},",
        hex(&nack_bytes)
    )
    .unwrap();
    writeln!(
        out,
        "  \"ack\": {{ \"sent\": 100, \"received\": 91, \"late\": 3, \"rtt_micros\": 31250, \"payload_hex\": \"{}\" }}",
        hex(&ack_bytes)
    )
    .unwrap();
    out.push_str("}\n");
    out
}

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("spec")
        .join("control-vectors.json")
}

#[test]
fn control_payloads_match_committed_vectors() {
    let rendered = render_json();
    let path = vectors_path();
    if std::env::var_os("FLUXCAST_BLESS").is_some() {
        std::fs::write(&path, &rendered).unwrap();
        return;
    }
    // Git may check text files out as CRLF on Windows. The vector format is
    // line-oriented JSON, so line-ending conversion must not make a canonical
    // vector test fail; the actual field bytes are asserted below.
    let committed = std::fs::read_to_string(&path)
        .expect("read spec/control-vectors.json")
        .replace("\r\n", "\n");
    assert_eq!(
        committed, rendered,
        "spec/control-vectors.json is stale; regenerate with FLUXCAST_BLESS=1"
    );
}

#[test]
fn control_payloads_round_trip() {
    let fec = sample_fec();
    assert_eq!(decode_fec_payload(&encode_fec_payload(&fec)), Some(fec));

    let nack = vec![1u32, 2, 4_000_000];
    assert_eq!(decode_nack_payload(&encode_nack_payload(&nack)), Some(nack));

    let feedback = TransportFeedback {
        sent: 100,
        received: 91,
        late: 3,
        rtt: Duration::from_micros(31_250),
    };
    assert_eq!(
        decode_feedback_payload(&encode_feedback_payload(feedback)),
        Some(feedback)
    );
}
