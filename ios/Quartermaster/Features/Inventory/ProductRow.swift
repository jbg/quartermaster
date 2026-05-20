import SwiftUI

struct ProductRow: View {
  let product: Product
  /// Batches displayed by the active filter (may be a subset of the product's stock).
  let visibleBatches: [StockBatch]
  /// Active batches for this product/location before applying expiry filters.
  /// Used to show a "... of <total>" contrast when a filter is hiding active stock.
  let allBatches: [StockBatch]
  let units: [Unit]

  var body: some View {
    HStack(spacing: 12) {
      productThumb
      VStack(alignment: .leading, spacing: 3) {
        Text(product.name)
          .font(.body.weight(.medium))
          .foregroundStyle(Color.quartermasterTextPrimary)
          .lineLimit(2)
        if let brand = product.brand, !brand.isEmpty {
          Text(brand)
            .font(.caption)
            .quartermasterMetadata()
            .lineLimit(1)
        }
        quantityLine
      }
      Spacer()
      ExpiryBadge(expiresOn: earliestExpiry)
    }
    .padding(.vertical, 2)
    .contentShape(Rectangle())
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
    .clipShape(RoundedRectangle(cornerRadius: QuartermasterRadius.sm))
    .overlay {
      RoundedRectangle(cornerRadius: QuartermasterRadius.sm)
        .stroke(Color.quartermasterBorder, lineWidth: 1)
    }
  }

  private var placeholder: some View {
    Image(systemName: icon)
      .foregroundStyle(Color.quartermasterTextMuted)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(Color.quartermasterSubtleSurface)
  }

  private var icon: String {
    switch product.family {
    case .mass: "scalemass"
    case .volume: "drop"
    case .count: "number"
    }
  }

  private var earliestExpiry: String? {
    visibleBatches.compactMap(\.expiresOn).min()
  }

  @ViewBuilder
  private var quantityLine: some View {
    let visibleTotal = UnitConversion.sum(
      visibleBatches, inUnit: product.preferredUnit, units: units)
    let allTotal = UnitConversion.sum(allBatches, inUnit: product.preferredUnit, units: units)
    let filterHidingSomething = allBatches.count != visibleBatches.count

    HStack(spacing: 6) {
      if let visibleTotal {
        if filterHidingSomething, let allTotal {
          Text("\(formatDecimal(visibleTotal)) \(product.preferredUnit) matching")
            .font(.subheadline.weight(.medium))
            .foregroundStyle(Color.quartermasterTextPrimary)
          Text("·")
            .quartermasterMetadata()
          Text("\(formatDecimal(allTotal)) \(product.preferredUnit) total")
            .font(.caption)
            .quartermasterMetadata()
        } else {
          Text("\(formatDecimal(visibleTotal)) \(product.preferredUnit)")
            .font(.subheadline.weight(.medium))
            .foregroundStyle(Color.quartermasterTextPrimary)
        }
      } else {
        Text("Mixed units")
          .font(.subheadline.weight(.medium))
          .quartermasterMetadata()
      }
      if filterHidingSomething {
        Text("· \(visibleBatches.count)/\(allBatches.count) batches")
          .font(.caption)
          .quartermasterMetadata()
      } else if visibleBatches.count > 1 {
        Text("· \(visibleBatches.count) batches")
          .font(.caption)
          .quartermasterMetadata()
      }
    }
  }

  private func formatDecimal(_ d: Decimal) -> String {
    var copy = d
    var rounded = Decimal()
    NSDecimalRound(&rounded, &copy, 3, .plain)
    return NSDecimalNumber(decimal: rounded).stringValue
  }
}
