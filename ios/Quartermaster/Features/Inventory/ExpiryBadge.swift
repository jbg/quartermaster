import SwiftUI

struct ExpiryBadge: View {
  @Environment(AppState.self) private var appState
  let expiresOn: String?

  var body: some View {
    Text(label)
      .font(.caption.weight(.medium))
      .foregroundStyle(foreground)
      .padding(.horizontal, 8)
      .padding(.vertical, 3)
      .background(background, in: Capsule())
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

  private var foreground: Color {
    switch severity {
    case .expired: .white
    case .soon: QuartermasterBrand.warningForeground
    case .ok: QuartermasterBrand.successForeground
    case .none: QuartermasterBrand.neutralForeground
    }
  }

  private var background: AnyShapeStyle {
    switch severity {
    case .expired: AnyShapeStyle(QuartermasterBrand.beetStrong)
    case .soon: AnyShapeStyle(QuartermasterBrand.warningBackground)
    case .ok: AnyShapeStyle(QuartermasterBrand.successBackground)
    case .none: AnyShapeStyle(QuartermasterBrand.neutralBackground)
    }
  }

  private enum Severity { case expired, soon, ok, none }
}
