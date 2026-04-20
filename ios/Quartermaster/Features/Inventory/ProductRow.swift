import SwiftUI

struct ProductRow: View {
    let product: Product
    let batches: [StockBatch]
    let units: [Unit]

    var body: some View {
        HStack(spacing: 12) {
            productThumb
            VStack(alignment: .leading, spacing: 2) {
                Text(product.displayTitle)
                    .font(.body)
                    .lineLimit(2)
                HStack(spacing: 6) {
                    Text(totalLabel)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.primary)
                    if batches.count > 1 {
                        Text("· \(batches.count) batches")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            Spacer()
            ExpiryBadge(expiresOn: earliestExpiry)
        }
        .padding(.vertical, 2)
    }

    private var productThumb: some View {
        Group {
            if let url = product.imageURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFit()
                    default:
                        placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: 40, height: 40)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private var placeholder: some View {
        Image(systemName: icon)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.secondary.opacity(0.1))
    }

    private var icon: String {
        switch product.family {
        case .mass: "scalemass"
        case .volume: "drop"
        case .count: "number"
        }
    }

    private var earliestExpiry: Date? {
        batches.compactMap(\.expiresOnDate).min()
    }

    private var totalLabel: String {
        if let total = UnitConversion.sum(batches, inUnit: product.preferredUnit, units: units) {
            return "\(formatDecimal(total)) \(product.preferredUnit)"
        }
        // Fallback when any batch is in an incompatible unit (shouldn't happen
        // given server-side family-match enforcement, but handle gracefully).
        return "Mixed units"
    }

    private func formatDecimal(_ d: Decimal) -> String {
        var copy = d
        var rounded = Decimal()
        NSDecimalRound(&rounded, &copy, 3, .plain)
        return NSDecimalNumber(decimal: rounded).stringValue
    }
}
