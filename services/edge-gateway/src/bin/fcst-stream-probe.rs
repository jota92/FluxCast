//! End-to-end probe for the Safari-compatible FCST stream transport.
//!
//! It fills the complete 1080p Visual State with a known RGB colour.  This is
//! useful for verifying an Edge deployment without accessing a real camera.

use anyhow::{Context, Result};
use fcst_protocol::{AtomType, HEADER_LEN, REGION_COUNT, VERSION};
use tokio::time::{Duration, sleep};
use wtransport::{ClientConfig, Endpoint};

const RAW_SURFACE_BYTES: usize = 1 + 32 * 24 * 3;
const ATOM_BYTES: usize = HEADER_LEN + RAW_SURFACE_BYTES;

fn surface_atom(region_id: u16, sequence: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut atom = vec![0_u8; ATOM_BYTES];
    atom[..2].copy_from_slice(&[0xfc, 0x01]);
    atom[2] = VERSION;
    atom[3] = AtomType::Surface as u8;
    atom[6..8].copy_from_slice(&(HEADER_LEN as u16).to_be_bytes());
    atom[8..12].copy_from_slice(&1_u32.to_be_bytes());
    atom[12..16].copy_from_slice(&sequence.to_be_bytes());
    atom[16..20].copy_from_slice(&sequence.to_be_bytes());
    atom[20..22].copy_from_slice(&region_id.to_be_bytes());
    atom[22] = 0;
    atom[23] = 1;
    atom[24..28].copy_from_slice(&sequence.to_be_bytes());
    atom[36..38].copy_from_slice(&120_u16.to_be_bytes());
    atom[38..40].copy_from_slice(&(RAW_SURFACE_BYTES as u16).to_be_bytes());
    atom[HEADER_LEN] = 0xff;
    for pixel in atom[HEADER_LEN + 1..].chunks_exact_mut(3) {
        pixel.copy_from_slice(&rgb);
    }
    atom
}

#[tokio::main]
async fn main() -> Result<()> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://flexcast-studio.eastasia.cloudapp.azure.com/fc".into());
    let endpoint =
        Endpoint::client(ClientConfig::default()).context("create WebTransport client")?;
    let connection = endpoint.connect(url).await.context("connect to Edge")?;
    let mut stream = connection.open_uni().await?.await?;
    let rgb = [0x25, 0x80, 0xf0];
    for region_id in 0..REGION_COUNT {
        let atom = surface_atom(region_id, u32::from(region_id) + 1, rgb);
        stream.write_all(&(ATOM_BYTES as u32).to_be_bytes()).await?;
        stream.write_all(&atom).await?;
    }
    // 30fps・約16Mbpsの動的サンプル。Safari互換ストリームの継続更新を検証する。
    let mut sequence = u32::from(REGION_COUNT) + 1;
    for frame in 0_u16..90 {
        for slot in 0_u16..35 {
            let region_id = (frame * 35 + slot) % REGION_COUNT;
            let sample_rgb = [
                frame.wrapping_mul(7) as u8,
                region_id.wrapping_mul(3) as u8,
                frame.wrapping_add(region_id).wrapping_mul(5) as u8,
            ];
            let atom = surface_atom(region_id, sequence, sample_rgb);
            stream.write_all(&(ATOM_BYTES as u32).to_be_bytes()).await?;
            stream.write_all(&atom).await?;
            sequence += 1;
        }
        sleep(Duration::from_millis(33)).await;
    }
    stream.finish().await?;
    println!(
        "fcst.stream_probe.sent initial_regions={REGION_COUNT} sample_frames=90 sample_updates=3150 rgb=#{:02x}{:02x}{:02x}",
        rgb[0], rgb[1], rgb[2]
    );
    Ok(())
}
