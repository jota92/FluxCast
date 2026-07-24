# FluxCast camera demo

Streams a local camera + microphone to a browser via FluxCast/FCDP, with an
Azure-hosted Flask site playing the result over HLS.

```
camera + mic → ffmpeg (H.264/AAC, MPEG-TS)
             → fluxcast-cli publish-ts  (FCDP/UDP, this is FluxCast transport)
             → Azure: fcdp_ts_receive.py (FluxCast Python SDK) → ffmpeg (HLS remux, no re-encode)
             → Flask (app.py) → browser <video> (hls.js / native HLS)
```

The receiver never decrypts or re-encodes media; ffmpeg only repackages the
MPEG-TS into HLS segments for browser playback.

## Server side (Azure)

Deployed to the `fluxcast-lab` VM as two systemd services:

- `fluxcast-receiver` — `run_receiver.sh` runs `fcdp_ts_receive.py | ffmpeg → HLS`
  in `/tmp/fluxcast-hls`, listening for FCDP on UDP `:19300`.
- `fluxcast-flask` — `app.py` serves the player and HLS segments on TCP `:8000`.

The NSG restricts both ports to the publisher's public IP.

## Publisher side (your Mac)

Find your devices: `ffmpeg -f avfoundation -list_devices true -i ""`.
Then, from the repo root (build once with `cargo build --release -p fluxcast-cli`):

```sh
ffmpeg -hide_banner -loglevel error \
  -f avfoundation -framerate 30 -video_size 640x480 -i "0:1" \
  -c:v libx264 -preset ultrafast -tune zerolatency -g 30 -pix_fmt yuv420p \
  -c:a aac -ar 44100 -f mpegts - \
  | ./target/release/fluxcast-cli publish-ts <AZURE_IP>:19300
```

`-i "0:1"` selects video device 0 and audio device 1. macOS prompts for camera
and microphone permission on first run. Open `http://<AZURE_IP>:8000/` to watch;
HLS adds ~3–6 s of latency.

## Local dry run (no camera, no Azure)

```sh
FLUXCAST_HLS_DIR=/tmp/hls bash demo/run_receiver.sh 127.0.0.1:19300 &
FLUXCAST_HLS_DIR=/tmp/hls PORT=8000 python3 demo/app.py &
ffmpeg -re -f lavfi -i testsrc2=size=640x360:rate=25 -f lavfi -i sine=frequency=440 \
  -c:v libx264 -preset ultrafast -tune zerolatency -g 25 -pix_fmt yuv420p -c:a aac -f mpegts - \
  | ./target/release/fluxcast-cli publish-ts 127.0.0.1:19300
# open http://127.0.0.1:8000/
```
