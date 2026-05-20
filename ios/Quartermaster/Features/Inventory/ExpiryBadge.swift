import SwiftUI

struct ExpiryBadge: View {
  @Environment(AppState.self) private var appState
  let expiresOn: String?

  var body: some View {
    Text(label)
      .font(.caption.weight(.semibold))
      .foregroundStyle(statusStyle.foreground)
      .padding(.horizontal, 8)
      .padding(.vertical, 3)
      .background(statusStyle.background, in: Capsule())
      .overlay {
        Capsule()
          .stroke(statusStyle.border, lineWidth: 1)
      }
  }

  private var label: String {
    guard let days = appState.householdDayDifference(for: expiresOn) else { return "No date" }
    if days < 0 {
      return "Expired"
    } else if days == 0 {
      return "Today"
    } else if days == 1 {
      return "Tomorrow"
    } else if days < 7 {
      return "\(days)d"
    } else {
      return appState.displayDate(for: expiresOn) ?? "No date"
    }
  }

  private var severity: Severity {
    guard let days = appState.householdDayDifference(for: expiresOn) else { return .none }
    if days < 0 { return .expired }
    if days < 7 { return .soon }
    return .ok
  }

  private var statusStyle: QuartermasterStatusStyle {
    switch severity {
    case .expired: .expiredStrong
    case .soon: .soon
    case .ok: .available
    case .none: .neutral
    }
  }

  private enum Severity { case expired, soon, ok, none }
}
