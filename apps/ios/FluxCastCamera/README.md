# FluxCast Camera for iPhone

This Xcode project sends the iPhone back camera as hardware-encoded H.264
access units over FCDP/UDP. It is a direct LAN diagnostic application: it does
not yet establish an encrypted session, perform NAT traversal, or carry audio.
Use it only on a network you trust while the full mobile connection manager is
being completed.

## Receive on this Mac

Start a local H.264 player before opening the iPhone app:

```sh
cargo run -p fluxcast-cli -- receive-h264 0.0.0.0:19100 | \
  ffplay -fflags nobuffer -flags low_delay -framedrop -f h264 -
```

Find this Mac's Wi-Fi IP address (for example, `192.168.1.10`) and enter it in
the app with port `19100`. The iPhone and Mac must be on the same Wi-Fi network.
Open `FluxCastCamera.xcodeproj` in Xcode, choose your signed iPhone, and run.
The first run asks for camera and local-network permission.

The iOS Simulator has no physical camera and is not a useful sender test.
