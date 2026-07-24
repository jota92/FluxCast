use std::env;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use fluxcast_core::{
    AccessUnit, ChannelModel, Delivered, FecPolicy, MAX_MEDIA_PAYLOAD, MediaKind, MediaReceiver,
    MediaSender, Reassembler, RelaySubscriptions, SecureUdpEndpoint, UdpEndpoint,
    discover_server_reflexive_candidate, fragment_access_unit, simulate_delivery,
    split_h264_annex_b, split_ogg_pages,
};
use fluxcast_proto::{Header, PacketType};
use fluxcast_security::Identity;

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = run(&args) {
        eprintln!("error: {error}");
        eprintln!("Run `fluxcast-cli help` for usage.");
        std::process::exit(2);
    }
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("send") => send(&args[1..]),
        Some("receive") => receive(&args[1..]),
        Some("relay") => relay(&args[1..]),
        Some("demo") => demo(),
        Some("simulate") => simulate(&args[1..]),
        Some("pipeline-demo") => pipeline_demo(&args[1..]),
        Some("secure-demo") => secure_demo(),
        Some("stun") => stun(&args[1..]),
        Some("send-h264") => send_h264(&args[1..]),
        Some("send-opus") => send_opus(&args[1..]),
        Some("receive-file") => receive_file(&args[1..]),
        Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => Err(format!("unknown command `{command}`").into()),
    }
}

fn stun(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [server] = args else {
        return Err("usage: fluxcast-cli stun <host:port>".into());
    };
    let server = server
        .to_socket_addrs()?
        .next()
        .ok_or("STUN server did not resolve to an address")?;
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    let candidate = discover_server_reflexive_candidate(&socket, server, Duration::from_secs(3))?;
    println!(
        "server-reflexive candidate: {} via {}",
        candidate.address, candidate.stun_server
    );
    Ok(())
}

fn secure_demo() -> Result<(), Box<dyn std::error::Error>> {
    let publisher = Identity::generate();
    let subscriber = Identity::generate();
    let initiator = publisher.begin_handshake(77);
    let (welcome, subscriber_session, authenticated_publisher) =
        subscriber.accept_handshake(initiator.hello(), Some(publisher.public_key()))?;
    assert_eq!(authenticated_publisher, publisher.public_key());
    let publisher_session = initiator.complete(&welcome, subscriber.public_key())?;
    let sender = SecureUdpEndpoint::bind("127.0.0.1:0".parse()?, publisher_session)?;
    let mut receiver = SecureUdpEndpoint::bind("127.0.0.1:0".parse()?, subscriber_session)?;
    let mut header = Header::new(PacketType::Media);
    header.session_id = 77;
    header.epoch = 0;
    header.sequence_number = 1;
    sender.send(receiver.local_addr()?, header, b"encrypted access unit")?;
    let until = Instant::now() + Duration::from_secs(1);
    let mut buffer = vec![0; 1200];
    while Instant::now() < until {
        if let Some((actual, payload, _)) = receiver.receive(&mut buffer)? {
            assert_eq!(actual.sequence_number, 1);
            assert_eq!(payload, b"encrypted access unit");
            println!("secure UDP demo succeeded: authenticated encrypted FCDP payload");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Err("timed out waiting for secure UDP packet".into())
}

fn relay(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [bind, session_id, subscribers @ ..] = args else {
        return Err(
            "usage: fluxcast-cli relay <bind-host:port> <session-id> <subscriber-host:port>..."
                .into(),
        );
    };
    if subscribers.is_empty() {
        return Err("relay requires at least one subscriber".into());
    }
    let endpoint = UdpEndpoint::bind(bind.parse()?)?;
    let session_id = session_id.parse()?;
    let mut registry = RelaySubscriptions::new();
    let lease = Instant::now() + Duration::from_secs(3600);
    for subscriber in subscribers {
        registry.subscribe(session_id, subscriber.parse()?, lease);
    }
    println!(
        "relay listening on {}; session {session_id} has {} subscribers",
        endpoint.local_addr()?,
        subscribers.len()
    );
    let mut buffer = vec![0; 1200];
    let mut next_metrics = Instant::now() + Duration::from_secs(10);
    loop {
        match endpoint.receive(&mut buffer)? {
            Some((header, length, _)) => {
                for subscriber in registry.recipients(header.session_id, Instant::now()) {
                    endpoint.send(subscriber, &buffer[..length])?;
                    registry.record_forward(length);
                }
            }
            None => thread::sleep(Duration::from_millis(1)),
        }
        if Instant::now() >= next_metrics {
            println!("relay metrics: {:?}", registry.metrics());
            next_metrics += Duration::from_secs(10);
        }
    }
}

fn send(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [destination, payload] = args else {
        return Err("usage: fluxcast-cli send <host:port> <text>".into());
    };
    let destination: SocketAddr = destination.parse()?;
    let endpoint = UdpEndpoint::bind("0.0.0.0:0".parse()?)?;
    let now = Instant::now();
    let unit = AccessUnit {
        stream_id: 1,
        frame_id: 1,
        kind: MediaKind::VideoKey,
        deadline: now + Duration::from_secs(2),
        bytes: payload.as_bytes().to_vec(),
    };
    let mut sequence = 1;
    for packet in fragment_access_unit(1, 1, &mut sequence, &unit, now)? {
        endpoint.send(destination, &packet.bytes)?;
    }
    println!("sent {} bytes to {destination}", payload.len());
    Ok(())
}

fn send_h264(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [destination, input] = args else {
        return Err("usage: fluxcast-cli send-h264 <host:port> <annex-b.h264>".into());
    };
    let bytes = std::fs::read(input)?;
    let units = split_h264_annex_b(&bytes)?;
    let media: Vec<(MediaKind, Vec<u8>)> = units
        .into_iter()
        .map(|nal| (nal.kind, nal.bytes.to_vec()))
        .collect();
    send_media(destination, &media, "H.264 NAL units")
}

fn send_opus(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [destination, input] = args else {
        return Err("usage: fluxcast-cli send-opus <host:port> <input.opus>".into());
    };
    let bytes = std::fs::read(input)?;
    let media: Vec<(MediaKind, Vec<u8>)> = split_ogg_pages(&bytes)?
        .into_iter()
        .map(|page| (MediaKind::Audio, page.to_vec()))
        .collect();
    send_media(destination, &media, "Ogg Opus pages")
}

fn send_media(
    destination: &str,
    units: &[(MediaKind, Vec<u8>)],
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let destination: SocketAddr = destination.parse()?;
    let endpoint = UdpEndpoint::bind("0.0.0.0:0".parse()?)?;
    let mut sequence = 1;
    for (index, (kind, bytes)) in units.iter().enumerate() {
        let now = Instant::now();
        let unit = AccessUnit {
            stream_id: 1,
            frame_id: u32::try_from(index + 1)?,
            kind: *kind,
            deadline: now + Duration::from_secs(2),
            bytes: bytes.clone(),
        };
        for packet in fragment_access_unit(1, 1, &mut sequence, &unit, now)? {
            endpoint.send(destination, &packet.bytes)?;
        }
    }
    println!("sent {} {label} to {destination}", units.len());
    Ok(())
}

fn receive_file(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [address, output] = args else {
        return Err("usage: fluxcast-cli receive-file <bind-host:port> <output>".into());
    };
    let endpoint = UdpEndpoint::bind(address.parse()?)?;
    let mut file = std::fs::File::create(output)?;
    println!(
        "writing received media to {output}; listening on {}",
        endpoint.local_addr()?
    );
    let until = Instant::now() + Duration::from_secs(30);
    let mut buffer = vec![0; 1200];
    let mut frames = Reassembler::new();
    while Instant::now() < until {
        match endpoint.receive(&mut buffer)? {
            Some((header, length, _)) => {
                let (_, payload) = Header::decode(&buffer[..length])?;
                if let Some(frame) = frames.push(header, payload, Instant::now())? {
                    file.write_all(&frame)?;
                    file.flush()?;
                }
            }
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
    file.flush()?;
    Ok(())
}

fn receive(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [address] = args else {
        return Err("usage: fluxcast-cli receive <bind-host:port>".into());
    };
    let endpoint = UdpEndpoint::bind(address.parse()?)?;
    println!("listening on {}", endpoint.local_addr()?);
    let until = Instant::now() + Duration::from_secs(30);
    let mut buffer = vec![0; 1200];
    let mut frames = Reassembler::new();
    while Instant::now() < until {
        match endpoint.receive(&mut buffer)? {
            Some((header, length, peer)) => {
                let (_, payload) = Header::decode(&buffer[..length])?;
                if let Some(frame) = frames.push(header, payload, Instant::now())? {
                    println!(
                        "received {} bytes from {peer}: {}",
                        frame.len(),
                        String::from_utf8_lossy(&frame)
                    );
                    std::io::stdout().flush()?;
                }
            }
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
    Ok(())
}

fn simulate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // usage: fluxcast-cli simulate [loss_rate] [frames] [seed] [reorder]
    let loss_rate: f32 = args.first().map_or(Ok(0.02), |value| value.parse())?;
    let frames: u32 = args.get(1).map_or(Ok(300), |value| value.parse())?;
    let seed: u64 = args.get(2).map_or(Ok(1), |value| value.parse())?;
    let reorder = args.get(3).is_none_or(|value| value != "in-order");
    if !(0.0..=1.0).contains(&loss_rate) {
        return Err("loss_rate must be between 0.0 and 1.0".into());
    }

    let now = Instant::now();
    let deadline = now + Duration::from_millis(200);
    // A representative GOP: one large keyframe followed by smaller delta frames.
    let units: Vec<AccessUnit> = (0..frames)
        .map(|frame_id| {
            let is_key = frame_id % 60 == 0;
            let bytes = if is_key {
                MAX_MEDIA_PAYLOAD * 3 + 40
            } else {
                MAX_MEDIA_PAYLOAD + 20
            };
            AccessUnit {
                stream_id: 1,
                frame_id,
                kind: if is_key {
                    MediaKind::VideoKey
                } else {
                    MediaKind::VideoDelta
                },
                deadline,
                bytes: vec![u8::try_from(frame_id % 251).unwrap_or(0); bytes],
            }
        })
        .collect();

    let model = ChannelModel {
        loss_rate,
        reorder,
        propagation: Duration::from_millis(15),
        seed,
    };
    let report = simulate_delivery(&units, 1, 0, &model, now)?;

    println!("FluxCast impairment simulation (deterministic, seed {seed})");
    println!(
        "  channel:          loss={:.1}% reorder={} propagation=15ms",
        loss_rate * 100.0,
        reorder
    );
    println!("  frames offered:   {}", report.frames_offered);
    println!(
        "  datagrams:        {} sent, {} lost ({:.2}%)",
        report.datagrams_sent,
        report.datagrams_lost,
        report.datagram_loss_rate() * 100.0
    );
    println!("  delivered clean:  {}", report.frames_delivered_clean);
    println!("  recovered by FEC: {}", report.frames_recovered_by_fec);
    println!("  dropped (late):   {}", report.frames_dropped_late);
    println!("  dropped (lost):   {}", report.frames_dropped_lost);
    println!(
        "  frame delivery:   {:.2}%   fec recovery: {:.2}%",
        report.frame_delivery_rate() * 100.0,
        report.fec_recovery_rate() * 100.0
    );
    Ok(())
}

fn pipeline_demo(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // usage: fluxcast-cli pipeline-demo [loss_rate] [frames]
    let loss_rate: f32 = args.first().map_or(Ok(0.15), |value| value.parse())?;
    let frames: u32 = args.get(1).map_or(Ok(120), |value| value.parse())?;
    if !(0.0..=1.0).contains(&loss_rate) {
        return Err("loss_rate must be between 0.0 and 1.0".into());
    }

    let now = Instant::now();
    let deadline = now + Duration::from_secs(2);
    let mut sender = MediaSender::new(1, 0, FecPolicy::PerFrame, 8192);
    let mut receiver = MediaReceiver::new();
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let drop_next = |state: &mut u64| {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        #[allow(clippy::cast_precision_loss)]
        let unit = (*state >> 40) as f32 / 16_777_216.0;
        unit < loss_rate
    };

    let (mut sent, mut lost, mut clean, mut recovered, mut nack_healed) =
        (0u32, 0u32, 0u32, 0u32, 0u32);

    for frame_id in 0..frames {
        // Every 30th frame is a keyframe; the rest are delta video.
        let is_key = frame_id % 30 == 0;
        let unit = AccessUnit {
            stream_id: 1,
            frame_id,
            kind: if is_key {
                MediaKind::VideoKey
            } else {
                MediaKind::VideoDelta
            },
            deadline,
            bytes: vec![
                u8::try_from(frame_id % 251).unwrap_or(0);
                if is_key {
                    MAX_MEDIA_PAYLOAD * 2 + 9
                } else {
                    MAX_MEDIA_PAYLOAD + 5
                }
            ],
        };
        let datagrams = sender.encode_access_unit(&unit, now)?;
        for datagram in &datagrams {
            sent += 1;
            if drop_next(&mut state) {
                lost += 1;
                continue;
            }
            let (header, payload) = Header::decode(&datagram.bytes)?;
            match receiver.accept(header, payload, now)? {
                Delivered::Clean(_) => clean += 1,
                Delivered::Recovered(_) => recovered += 1,
                Delivered::Pending | Delivered::Duplicate => {}
            }
        }
        // One NACK round; only cached audio/keyframe fragments come back.
        let nacks = receiver.nack_requests(now);
        for datagram in sender.on_nack(&nacks, now) {
            sent += 1;
            if drop_next(&mut state) {
                lost += 1;
                continue;
            }
            let (header, payload) = Header::decode(&datagram.bytes)?;
            if matches!(
                receiver.accept(header, payload, now)?,
                Delivered::Clean(_) | Delivered::Recovered(_)
            ) {
                nack_healed += 1;
            }
        }
    }
    // Frames not completed by clean delivery, FEC, or NACK are undelivered.
    let dropped = frames - clean - recovered - nack_healed;

    println!("FluxCast M1 pipeline demo (FEC=PerFrame, one NACK round)");
    println!("  channel loss:        {:.1}%", loss_rate * 100.0);
    println!("  frames:              {frames}");
    println!("  datagrams sent/lost: {sent} / {lost}");
    println!("  delivered clean:     {clean}");
    println!("  recovered by FEC:    {recovered}");
    println!("  recovered by NACK:   {nack_healed}");
    println!("  undelivered:         {dropped}");
    Ok(())
}

fn demo() -> Result<(), Box<dyn std::error::Error>> {
    let receiver = UdpEndpoint::bind("127.0.0.1:0".parse()?)?;
    let destination = receiver.local_addr()?;
    let sender = UdpEndpoint::bind("127.0.0.1:0".parse()?)?;
    let now = Instant::now();
    let body = vec![b'x'; MAX_MEDIA_PAYLOAD + 32];
    let unit = AccessUnit {
        stream_id: 1,
        frame_id: 7,
        kind: MediaKind::VideoKey,
        deadline: now + Duration::from_secs(1),
        bytes: body.clone(),
    };
    let mut sequence = 1;
    for packet in fragment_access_unit(99, 1, &mut sequence, &unit, now)? {
        sender.send(destination, &packet.bytes)?;
    }
    let mut buffer = vec![0; 1200];
    let mut reassembler = Reassembler::new();
    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until {
        if let Some((header, length, _)) = receiver.receive(&mut buffer)? {
            let (_, payload) = Header::decode(&buffer[..length])?;
            if let Some(frame) = reassembler.push(header, payload, Instant::now())? {
                assert_eq!(frame, body);
                println!("local UDP demo succeeded: {} byte access unit", frame.len());
                return Ok(());
            }
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
    Err("timed out waiting for local UDP packets".into())
}

fn print_help() {
    println!(
        "FluxCast pre-alpha diagnostic CLI\n\nCommands:\n  send <host:port> <text>                 send a deadline-aware test access unit\n  send-h264 <host:port> <annex-b.h264>    send H.264 Annex-B NAL units\n  send-opus <host:port> <input.opus>      send Ogg Opus pages\n  receive <host:port>                     receive test access units for 30 seconds\n  receive-file <bind> <output>            recover media stream to a file\n  relay <bind> <session-id> <subscriber>... fan out one session to viewers\n  stun <host:port>                        discover a server-reflexive UDP candidate\n  demo                                    run an in-process UDP fragmentation/reassembly demo\n  simulate [loss] [frames] [seed] [in-order] deterministic loss/reorder/deadline report\n  pipeline-demo [loss] [frames]           end-to-end sender/FEC/NACK/receiver recovery\n  secure-demo                             run an authenticated encrypted UDP demo"
    );
}
