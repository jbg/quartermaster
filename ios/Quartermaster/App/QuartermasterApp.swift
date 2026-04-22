import SwiftUI
import UIKit
import UserNotifications

@main
struct QuartermasterApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            AppRoot()
                .environment(appState)
                .task {
                    appDelegate.appState = appState
                    await appState.bootstrap()
                }
                .onOpenURL { url in
                    appState.handleIncomingURL(url)
                }
                .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { activity in
                    appState.handleIncomingUserActivity(activity)
                }
                .onChange(of: scenePhase) { _, phase in
                    guard phase == .active else { return }
                    Task {
                        await appState.refreshNotificationAuthorization()
                        await appState.registerCurrentDevice()
                        await appState.refreshRemindersSilently()
                    }
                }
        }
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    weak var appState: AppState?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        Task { await appState?.updatePushToken(deviceToken) }
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        Task { @MainActor in
            appState?.handlePushRegistrationFailure(error)
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let payload = ReminderPushPayload(userInfo: notification.request.content.userInfo)
        completionHandler([])
        guard let payload else { return }
        Task { @MainActor [weak self] in
            await self?.appState?.handleRemoteNotification(payload, opened: false)
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let payload = ReminderPushPayload(userInfo: response.notification.request.content.userInfo)
        completionHandler()
        guard let payload else { return }
        Task { @MainActor [weak self] in
            await self?.appState?.handleRemoteNotification(payload, opened: true)
        }
    }
}
