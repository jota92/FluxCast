import SwiftUI

@main
struct FluxCastCameraApp: App {
    var body: some Scene { WindowGroup { ContentView() } }
}

struct ContentView: View {
    @StateObject private var publisher = CameraPublisher()
    @State private var host = "192.168.1.10"
    @State private var port = "19100"

    var body: some View {
        VStack(spacing: 16) {
            CameraPreview(session: publisher.session)
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .aspectRatio(9.0 / 16.0, contentMode: .fit)
            TextField("Mac IP address", text: $host)
                .textInputAutocapitalization(.never).keyboardType(.numbersAndPunctuation).textFieldStyle(.roundedBorder)
            TextField("UDP port", text: $port).keyboardType(.numberPad).textFieldStyle(.roundedBorder)
            Text(publisher.status).font(.footnote).foregroundStyle(publisher.isPublishing ? .green : .secondary)
            Button(publisher.isPublishing ? "Stop" : "Start camera stream") {
                if publisher.isPublishing { publisher.stop() }
                else if let value = UInt16(port) { publisher.start(host: host, port: value) }
                else { publisher.status = "Enter a UDP port between 1 and 65535." }
            }.buttonStyle(.borderedProminent)
        }.padding()
    }
}
