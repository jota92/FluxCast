use std::env;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use fluxcast_core::{
    AccessUnit, MAX_MEDIA_PAYLOAD, MediaKind, Reassembler, SecureUdpEndpoint, UdpEndpoint,
    discover_server_reflexive_candidate, fragment_access_unit,
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
        Some("secure-demo") => secure_demo(),
        Some("stun") => stun(&args[1..]),
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
    let [bind, subscriber] = args else {
        return Err("usage: fluxcast-cli relay <bind-host:port> <subscriber-host:port>".into());
    };
    let endpoint = UdpEndpoint::bind(bind.parse()?)?;
    let subscriber: SocketAddr = subscriber.parse()?;
    println!(
        "relay listening on {}; forwarding to {subscriber}",
        endpoint.local_addr()?
    );
    let mut buffer = vec![0; 1200];
    loop {
        match endpoint.receive(&mut buffer)? {
            Some((_, length, _)) => {
                endpoint.send(subscriber, &buffer[..length])?;
            }
            None => thread::sleep(Duration::from_millis(1)),
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
                }
            }
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
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
        "FluxCast pre-alpha diagnostic CLI\n\nCommands:\n  send <host:port> <text>                 send a deadline-aware test access unit\n  receive <host:port>                     receive test access units for 30 seconds\n  relay <bind> <subscriber-host:port>     validate and forward FCDP datagrams\n  stun <host:port>                        discover a server-reflexive UDP candidate\n  demo                                    run an in-process UDP fragmentation/reassembly demo\n  secure-demo                             run an authenticated encrypted UDP demo"
    );
}
