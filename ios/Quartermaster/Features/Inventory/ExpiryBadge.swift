import SwiftUI

struct ExpiryBadge: View {
    let expiresOn: Date?

    var body: some View {
        Text(label)
            .font(.caption.weight(.medium))
            .foregroundStyle(foreground)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(background, in: Capsule())
    }

    private var label: String {
        guard let expiresOn else { return "No date" }
        let days = Calendar.current.dateComponents([.day], from: Calendar.current.startOfDay(for: .now), to: Calendar.current.startOfDay(for: expiresOn)).day ?? 0
        if days < 0 {
            return "Expired"
        } else if days == 0 {
            return "Today"
        } else if days == 1 {
            return "Tomorrow"
        } else if days < 7 {
            return "\(days)d"
        } else {
            return Self.shortDate.string(from: expiresOn)
        }
    }

    private var severity: Severity {
        guard let expiresOn else { return .none }
        let days = Calendar.current.dateComponents([.day], from: Calendar.current.startOfDay(for: .now), to: Calendar.current.startOfDay(for: expiresOn)).day ?? 0
        if days < 0 { return .expired }
        if days < 7 { return .soon }
        return .ok
    }

    private var foreground: Color {
        switch severity {
        case .expired: .white
        case .soon: .orange
        case .ok: .green
        case .none: .secondary
        }
    }

    private var background: AnyShapeStyle {
        switch severity {
        case .expired: AnyShapeStyle(Color.red)
        case .soon: AnyShapeStyle(Color.orange.opacity(0.15))
        case .ok: AnyShapeStyle(Color.green.opacity(0.15))
        case .none: AnyShapeStyle(Color.secondary.opacity(0.1))
        }
    }

    private enum Severity { case expired, soon, ok, none }

    private static let shortDate: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        return f
    }()
}
