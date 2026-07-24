#!/usr/bin/env bash
set -euo pipefail

port=19100
cargo run -q -p fluxcast-cli -- receive "127.0.0.1:${port}" > /tmp/fluxcast-receive.log 2>&1 &
receiver=$!
trap 'kill "$receiver" 2>/dev/null || true' EXIT
sleep 1
python3 examples/python/send_fcdp.py 127.0.0.1 "$port" python
node examples/node/send-fcdp.mjs 127.0.0.1 "$port" node
cc -std=c17 -Wall -Wextra -Werror examples/c/send_fcdp.c -o /tmp/fluxcast-c-sender
/tmp/fluxcast-c-sender 127.0.0.1 "$port" c
sleep 1
rg -q 'python' /tmp/fluxcast-receive.log
rg -q 'node' /tmp/fluxcast-receive.log
rg -q 'c$' /tmp/fluxcast-receive.log
echo "Python, Node.js, and C interoperability passed"
