# Local development

FlexCast requires a browser secure context. The sender does not provide an HTTP
fallback because camera capture and WebTransport must be tested under their
real security constraints.

1. Create a local certificate trusted by your device (for example, with a
   local development CA). Do not commit certificate keys.
2. Start the Edge with the certificate and key:

   ```sh
   cargo run -p edge-gateway -- --bind 0.0.0.0:4433 \
     --cert certs/localhost.pem --key certs/localhost-key.pem
   ```

3. Start the sender site with `pnpm install && pnpm dev` and serve it through
   HTTPS. For a phone test, the certificate must include the LAN hostname or
   public hostname used by the phone.

4. Set the Edge URL in the sender UI to `https://<edge-host>:4433/fc`.

The Edge accepts one reliable bidirectional control stream and media through
WebTransport datagrams. It never writes raw visual or audio payloads to logs.
