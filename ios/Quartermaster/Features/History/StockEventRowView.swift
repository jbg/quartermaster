import SwiftUI

struct StockEventRowView: View {
    let event: StockEvent

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

    private static let relative: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()
}
