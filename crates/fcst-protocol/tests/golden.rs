use fcst_protocol::{AtomType, Header, Surface};

#[test]
fn decodes_typescript_surface_golden_vector() {
    let hex = include_str!("../../../tests/protocol/surface_001.hex")
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let (header, payload) = Header::decode(&bytes).unwrap();
    assert_eq!(header.atom_type, AtomType::Surface);
    assert_eq!(header.region_id, 2699);
    assert_eq!(header.state_id, 8);
    assert_eq!(Surface::decode(payload).unwrap().luma, [12; 48]);
}
