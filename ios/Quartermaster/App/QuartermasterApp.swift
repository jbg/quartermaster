import SwiftUI

@main
struct QuartermasterApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            AppRoot()
                .environment(appState)
                .task { await appState.bootstrap() }
                .onOpenURL { url in
                    appState.handleIncomingURL(url)
                }
                .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { activity in
                    appState.handleIncomingUserActivity(activity)
                }
                .onChange(of: scenePhase) { _, phase in
                    guard phase == .active else { return }
                    Task {
                        await appState.registerCurrentDevice()
                        await appState.syncDueReminders()
                    }
                }
        }
    }
}
