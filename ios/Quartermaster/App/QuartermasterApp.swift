import SwiftUI

@main
struct QuartermasterApp: App {
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            AppRoot()
                .environment(appState)
                .task { await appState.bootstrap() }
                .onOpenURL { url in
                    appState.handleIncomingURL(url)
                }
        }
    }
}
