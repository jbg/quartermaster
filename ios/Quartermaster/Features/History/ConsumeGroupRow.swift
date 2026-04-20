import SwiftUI

/// Collapsed summary row for a set of events sharing a `consume_request_id` —
/// i.e. one `POST /stock/consume` that fanned out across N batches. Tap to
/// expand; expanded state is owned by the parent so it survives list
/// reloads.
struct ConsumeGroupRow: View {
    let requestID: UUID
    let events: [StockEvent]
    let preferredUnit: String
    let units: [Unit]
    @Binding var expandedGroups: Set<UUID>

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                toggle()
            } label: {
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: "fork.knife")
                        .font(.title3)
                        .foregroundStyle(.blue)
                        .frame(width: 28, height: 28)
                        .background(Color.blue.opacity(0.12))
                        .clipShape(Circle())

                    VStack(alignment: .leading, spacing: 2) {
                        Text("Consumed")
                            .font(.subheadline.weight(.semibold))
                        Text(summaryLine)
                            .font(.body)
                            .lineLimit(2)
                        HStack(spacing: 4) {
                            if let actor = events.first?.createdByUsername {
                                Text(actor)
                            }
                            if let date = events.first?.createdAtDate {
                                if events.first?.createdByUsername != nil { Text("·") }
                                Text(Self.relative.localizedString(for: date, relativeTo: .now))
                            }
                        }
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }

                    Spacer()

                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .buttonStyle(.plain)

            if isExpanded {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(events) { event in
                        HStack {
                            Rectangle()
                                .frame(width: 2)
                                .foregroundStyle(Color.blue.opacity(0.3))
                            StockEventRowView(event: event, showExpiry: true)
                        }
                        .padding(.leading, 36)
                    }
                }
            }
        }
        .padding(.vertical, 2)
    }

    private var isExpanded: Bool {
        expandedGroups.contains(requestID)
    }

    private func toggle() {
        if isExpanded {
            expandedGroups.remove(requestID)
        } else {
            expandedGroups.insert(requestID)
        }
    }

    /// Human summary: "400 ml of Whole milk across 2 batches".
    private var summaryLine: String {
        let productName = events.first?.product.displayTitle ?? "stock"
        let count = events.count
        let batchesPhrase = count == 1 ? "1 batch" : "\(count) batches"

        let total = events.reduce(Decimal.zero) { partial, event in
            guard
                let delta = Decimal(string: event.quantityDelta),
                let converted = UnitConversion.convert(
                    delta.magnitude,
                    fromCode: event.unit,
                    toCode: preferredUnit,
                    units: units,
                )
            else { return partial }
            return partial + converted
        }

        if total > .zero {
            return "\(Self.format(total)) \(preferredUnit) of \(productName) across \(batchesPhrase)"
        }
        // Fallback if any event was in an incompatible unit.
        return "\(productName) across \(batchesPhrase)"
    }

    private static func format(_ d: Decimal) -> String {
        var copy = d
        var rounded = Decimal()
        NSDecimalRound(&rounded, &copy, 3, .plain)
        return NSDecimalNumber(decimal: rounded).stringValue
    }

    nonisolated(unsafe) private static let relative: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f
    }()
}

private extension Decimal {
    var magnitude: Decimal {
        self < .zero ? -self : self
    }
}
