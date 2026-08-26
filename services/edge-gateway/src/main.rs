#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use clap::Parser;
use fcst_protocol::{AtomType, Header, Surface};
use renderer::FrameRenderer;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use visual_state::VisualState;
use wtransport::{Endpoint, Identity, ServerConfig, endpoint::IncomingSession};

#[derive(Parser, Debug)]
#[command(about = "FlexCast FCST WebTransport Edge")]
struct Args {
    #[arg(long, default_value_t = 4433)]
    port: u16,
    #[arg(long)]
    cert: String,
    #[arg(long)]
    key: String,
}

#[derive(Default)]
struct EdgeMetrics {
    received_datagrams: u64,
    invalid_atoms: u64,
    replayed_atoms: u64,
    applied_atoms: u64,
    rendered_frames: u64,
}
struct Shared {
    state: Mutex<VisualState>,
    metrics: Mutex<EdgeMetrics>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let identity = Identity::load_pemfiles(&args.cert, &args.key)
        .await
        .context("load TLS identity")?;
    let config = ServerConfig::builder()
        .with_bind_default(args.port)
        .with_identity(identity)
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .build();
    let endpoint = Endpoint::server(config).context("create WebTransport endpoint")?;
    let shared = Arc::new(Shared {
        state: Mutex::new(VisualState::new()),
        metrics: Mutex::new(EdgeMetrics::default()),
    });
    tokio::spawn(render_clock(Arc::clone(&shared)));
    eprintln!(
        "flexcast.edge.started port={} path=/fc",
        endpoint.local_addr()?.port()
    );
    loop {
        let incoming = endpoint.accept().await;
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            if let Err(error) = handle(incoming, shared).await {
                eprintln!("flexcast.edge.connection_closed reason={error}");
            }
        });
    }
}

async fn render_clock(shared: Arc<Shared>) {
    let mut interval = tokio::time::interval(Duration::from_nanos(33_333_333));
    let mut renderer = FrameRenderer::new(Instant::now());
    loop {
        interval.tick().await;
        let now = Instant::now();
        let state = shared.state.lock().await;
        let _ = renderer.render(&state, now);
        drop(state);
        shared.metrics.lock().await.rendered_frames = renderer.frames();
    }
}

async fn handle(incoming: IncomingSession, shared: Arc<Shared>) -> Result<()> {
    let request = incoming.await?;
    if request.path() != "/fc" {
        anyhow::bail!("unexpected WebTransport path");
    }
    let connection = request.accept().await?;
    let mut last_by_epoch: HashMap<u32, u32> = HashMap::new();
    loop {
        tokio::select! {
            result = connection.accept_bi() => {
                let (mut send, mut recv) = result?;
                let mut length = [0_u8; 4];
                if recv.read_exact(&mut length).await.is_ok() {
                    let size = u32::from_be_bytes(length) as usize;
                    if size <= 16_384 { let mut body = vec![0_u8; size]; if recv.read_exact(&mut body).await.is_ok() { let accepted = b"{\"type\":\"SESSION_ACCEPT\",\"protocol\":\"FCST/1\"}"; send.write_all(&(accepted.len() as u32).to_be_bytes()).await?; send.write_all(accepted).await?; } }
                }
            }
            result = connection.receive_datagram() => {
                let datagram = result?;
                let now = Instant::now();
                let bytes = datagram.as_ref();
                let Ok((header, payload)) = Header::decode(bytes) else { shared.metrics.lock().await.invalid_atoms += 1; continue; };
                { let mut metrics = shared.metrics.lock().await; metrics.received_datagrams += 1; }
                let prior = last_by_epoch.entry(header.session_epoch).or_insert(0);
                if header.atom_sequence <= *prior { shared.metrics.lock().await.replayed_atoms += 1; continue; }
                *prior = header.atom_sequence;
                // capture_time_ms is a sender monotonic timestamp. TTL enforcement is
                // authoritative once clock synchronization/digests are introduced.
                if header.atom_type == AtomType::Surface || header.atom_type == AtomType::Refresh || header.atom_type == AtomType::Repair {
                    let Ok(surface) = Surface::decode(payload) else { shared.metrics.lock().await.invalid_atoms += 1; continue; };
                    let mut state = shared.state.lock().await;
                    if state.apply_surface(header, surface, now) { shared.metrics.lock().await.applied_atoms += 1; }
                }
            }
        }
    }
}
