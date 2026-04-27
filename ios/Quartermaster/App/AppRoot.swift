import SwiftUI

struct AppRoot: View {
  @Environment(AppState.self) private var appState

  var body: some View {
    Group {
      switch appState.phase {
      case .launching:
        LaunchView()
      case .launchFailed(let message):
        LaunchFailureView(message: message)
      case .unauthenticated:
        OnboardingView()
      case .authenticated(let me):
        if me.currentHouseholdSummary != nil {
          MainTabView()
        } else {
          NavigationStack {
            NoHouseholdView(me: me)
          }
        }
      }
    }
    .alert(
      appState.activeReminder?.displayTitle
        ?? NSLocalizedString("REMINDER_ALERT_TITLE", comment: ""),
      isPresented: Binding(
        get: { appState.activeReminder != nil },
        set: { _ in }
      ),
      presenting: appState.activeReminder
    ) { _ in
      Button(NSLocalizedString("REMINDER_ALERT_OPEN", comment: "")) {
        appState.openActiveReminder()
      }
      Button(NSLocalizedString("REMINDER_ALERT_DISMISS", comment: ""), role: .cancel) {
        appState.dismissActiveReminder()
      }
    } message: { reminder in
      Text(reminder.displayBody)
    }
  }
}

private struct LaunchView: View {
  var body: some View {
    VStack(spacing: 16) {
      Image(systemName: "shippingbox")
        .font(.system(size: 56, weight: .regular))
        .foregroundStyle(QuartermasterBrand.green800)
      Text("Quartermaster")
        .font(.title.weight(.semibold))
      ProgressView()
        .padding(.top, 24)
    }
    .foregroundStyle(.primary)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(.background)
  }
}

private struct LaunchFailureView: View {
  @Environment(AppState.self) private var appState

  let message: String

  var body: some View {
    VStack(spacing: 16) {
      Image(systemName: "wifi.exclamationmark")
        .font(.system(size: 56, weight: .regular))
        .foregroundStyle(QuartermasterBrand.beetStrong)
      Text("Couldn't resume session")
        .font(.title.weight(.semibold))
      Text(message)
        .font(.body)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)
      Button("Try again") {
        appState.phase = .launching
        Task { await appState.refreshMe() }
      }
      .buttonStyle(.borderedProminent)
      Button("Sign out") {
        Task { await appState.logout() }
      }
      .buttonStyle(.bordered)
    }
    .padding(.horizontal, 24)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(.background)
  }
}
