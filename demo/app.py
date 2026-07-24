#!/usr/bin/env python3
"""FluxCast camera demo site.

Serves a small page that plays the HLS stream produced by the FluxCast receive
pipeline (fcdp_ts_receive.py -> ffmpeg -> HLS). Flask only serves the player and
the HLS segments; the live media travels from the camera to this host over
FluxCast/FCDP, and this process never re-encodes it.
"""
import os

from flask import Flask, Response, render_template, send_from_directory

HLS_DIR = os.environ.get("FLUXCAST_HLS_DIR", "/tmp/fluxcast-hls")
app = Flask(__name__)


@app.route("/")
def index() -> str:
    return render_template("index.html")


@app.route("/healthz")
def healthz() -> Response:
    ready = os.path.exists(os.path.join(HLS_DIR, "stream.m3u8"))
    return Response("ready\n" if ready else "starting\n", mimetype="text/plain")


@app.route("/hls/<path:name>")
def hls(name: str) -> Response:
    response = send_from_directory(HLS_DIR, name, conditional=True)
    # Live HLS: never cache the playlist or segments.
    response.headers["Cache-Control"] = "no-store, max-age=0"
    if name.endswith(".m3u8"):
        response.headers["Content-Type"] = "application/vnd.apple.mpegurl"
    elif name.endswith(".ts"):
        response.headers["Content-Type"] = "video/mp2t"
    return response


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=int(os.environ.get("PORT", "8000")))
