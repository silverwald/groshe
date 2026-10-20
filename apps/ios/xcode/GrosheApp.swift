import SwiftUI

@main
struct GrosheApp: App {
    @Environment(\.scenePhase) private var scenePhase

    init() {
        _ = GrosheBridge.shared.initialize()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .onChange(of: scenePhase) { phase in
            switch phase {
            case .active:
                GrosheBridge.shared.onForeground()
            case .background:
                GrosheBridge.shared.onBackground()
            default:
                break
            }
        }
    }
}

struct ContentView: View {
    var body: some View {
        VStack(spacing: 16) {
            Text("Groshe P2P Grocery List")
                .font(.headline)
            Text("Slint UI Event Loop Active")
                .font(.subheadline)
                .foregroundColor(.secondary)
        }
        .padding()
        .onAppear {
            DispatchQueue.global(qos: .userInitiated).async {
                GrosheBridge.shared.run()
            }
        }
    }
}
