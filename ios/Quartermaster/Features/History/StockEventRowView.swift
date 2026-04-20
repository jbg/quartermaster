import SwiftUI

struct StockEventRowView: View {
    let event: StockEvent
    /// When true, the expiry badge alongside the quantity is suppressed
    /// (useful for the collapsed child rows inside a consume group, where
    /// the parent already carries the expiry context).
    var showExpiry: Bool = true

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: event.eventType.systemImage)
                .font(.title3)
                .foregroundStyle(accent)
                .frame(width: 28, height: 28)
                .background(accent.opacity(0.12))
                .clipShape(Circle())

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(event.eventType.displayLabel)
                        .font(.subheadline.weight(.semibold))
                    Text(quantityLabel)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(accent)
                    if showExpiry, event.batchExpiresOn != nil {
                        ExpiryBadge(expiresOn: event.batchExpiresOnDate)
                    }
                }
                Text(productLine)
                    .font(.body)
                    .foregroundStyle(event.product.isDeleted ? .secondary : .primary)
                    .lineLimit(2)
                HStack(spacing: 4) {
                    if let actor = event.createdByUsername {
                        Text(actor)
                    }
                    if let date = event.createdAtDate {
                        if event.createdByUsername != nil { Text("·") }
                        Text(Self.relative.localizedString(for: date, relativeTo: .now))
                    }
                    if event.product.isDeleted {
                        if event.createdByUsername != nil || event.createdAtDate != nil {
                            Text("·")
                        }
                        Text("product deleted")
                            .italic()
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }

    private var productLine: String {
        event.product.displayTitle
    }

    private var quantityLabel: String {
        "\(event.quantityDelta) \(event.unit)"
    }

    private var accent: Color {
        switch event.eventType {
        case .add, .restore: .green
        case .consume: .blue
        case .adjust: .orange
        case .discard: .red
        }
    }

    nonisolated(unsafe) private static let relative: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()
}
