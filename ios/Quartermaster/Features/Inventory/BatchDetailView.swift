import SwiftUI

/// Read-only detail view reached by tapping a row in `StockHistoryView`.
/// Loads the current state of the batch, shows product + location + expiry
/// context, and surfaces the right action: "Open in Inventory" for active
/// batches (deep-links via `AppState.pendingInventoryTarget`) or "Restore"
/// for discarded ones.
struct BatchDetailView: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    let batchID: UUID

    enum LoadState {
        case loading
        case loaded(StockBatch, Location?)
        case failed(String)
    }

    @State private var state: LoadState = .loading
    @State private var locations: [Location] = []
    @State private var isActing = false
    @State private var actionError: String?
    /// Bump after any action that should refresh the embedded mini-history.
    /// Applied as `.id(historyReloadToken)` on `MiniBatchHistory` so SwiftUI
    /// rebuilds the view and its `.task` re-runs.
    @State private var historyReloadToken: Int = 0

    var body: some View {
        Group {
            switch state {
            case .loading:
                ProgressView("Loading…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .loaded(let batch, let location):
                loaded(batch: batch, location: location)
            case .failed(let msg):
                ContentUnavailableView {
                    Label("Couldn't load batch", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(msg)
                } actions: {
                    Button("Try again") { Task { await load() } }
                        .buttonStyle(.borderedProminent)
                }
            }
        }
        .navigationTitle("Batch")
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
        .alert("Couldn't complete action", isPresented: Binding(
            get: { actionError != nil },
            set: { if !$0 { actionError = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(actionError ?? "")
        }
    }

    @ViewBuilder
    private func loaded(batch: StockBatch, location: Location?) -> some View {
        Form {
            Section {
                HStack(spacing: 12) {
                    productThumb(batch.product)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(batch.product.displayTitle)
                            .font(.headline)
                            .foregroundStyle(batch.product.isDeleted ? .secondary : .primary)
                        HStack(spacing: 4) {
                            Text(batch.product.family.displayName)
                            if batch.product.isDeleted {
                                Text("· product deleted").italic()
                            }
                        }
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                }
            }

            Section("Current state") {
                LabeledContent("Quantity") {
                    if isDepleted(batch) {
                        Text("Depleted")
                            .foregroundStyle(.secondary)
                    } else {
                        Text("\(batch.quantity) \(batch.unit)")
                            .fontWeight(.medium)
                    }
                }
                LabeledContent("Initial", value: "\(batch.initialQuantity) \(batch.unit)")
                if let location {
                    LabeledContent("Location", value: location.name)
                }
                LabeledContent("Expires") {
                    ExpiryBadge(expiresOn: batch.expiresOnDate)
                }
                if let opened = batch.openedOnDate {
                    LabeledContent("Opened", value: Self.shortDate.string(from: opened))
                }
                if let note = batch.note, !note.isEmpty {
                    LabeledContent("Note", value: note)
                }
            }

            Section("Recent history for this batch") {
                MiniBatchHistory(batchID: batch.id)
                    .id(historyReloadToken)
            }

            Section {
                if isDepleted(batch) {
                    Button {
                        Task { await restore(batch) }
                    } label: {
                        if isActing {
                            ProgressView()
                        } else {
                            Label("Restore (undo discard)", systemImage: "arrow.uturn.backward")
                        }
                    }
                    .disabled(isActing)
                }
                if let location {
                    // Always available. For active batches the target sheet
                    // flashes the row we came from; for depleted ones there's
                    // nothing to flash (the batch isn't in the active list),
                    // but the sheet still gives useful "what's actually here
                    // now" context for the same product + location.
                    Button {
                        openInInventory(product: batch.product, location: location, batch: batch)
                    } label: {
                        Label("Open in Inventory", systemImage: "arrow.up.right.square")
                    }
                }
            }
        }
    }

    private func productThumb(_ product: Product) -> some View {
        Group {
            if let url = product.imageURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFit()
                    default:
                        Color.secondary.opacity(0.1)
                    }
                }
            } else {
                Image(systemName: icon(for: product.family))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color.secondary.opacity(0.1))
            }
        }
        .frame(width: 48, height: 48)
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func icon(for family: ProductFamily) -> String {
        switch family {
        case .mass: "scalemass"
        case .volume: "drop"
        case .count: "number"
        }
    }

    private func isDepleted(_ batch: StockBatch) -> Bool {
        // A batch is "depleted" in the UI sense when its quantity is zero.
        // The server exposes `depleted_at` only on the DB row, so infer from
        // the cached quantity here.
        Decimal(string: batch.quantity).map { $0 <= .zero } ?? false
    }

    private func load() async {
        state = .loading
        if locations.isEmpty {
            locations = (try? await appState.api.locations()) ?? []
        }
        do {
            let batch = try await appState.api.getStock(id: batchID)
            let location = locations.first(where: { $0.id == batch.locationID })
            state = .loaded(batch, location)
        } catch let err as APIError {
            state = .failed(err.userFacingMessage)
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    private func restore(_ batch: StockBatch) async {
        isActing = true
        defer { isActing = false }
        do {
            _ = try await appState.api.restoreStock(id: batch.id)
            await load()
            historyReloadToken &+= 1
        } catch let err as APIError {
            actionError = err.userFacingMessage
        } catch {
            actionError = error.localizedDescription
        }
    }

    private func openInInventory(product: Product, location: Location, batch: StockBatch) {
        // Highlight only makes sense when the originating batch is actually
        // in the active list — depleted batches wouldn't be there, so the
        // flash would no-op and confuse the eye. Send nil in that case.
        appState.pendingInventoryTarget = InventoryTarget(
            productID: product.id,
            locationID: location.id,
            highlightBatchID: isDepleted(batch) ? nil : batch.id,
        )
        // Dismiss the containing sheet stack so the deep-link can resolve
        // cleanly on the Inventory tab.
        dismiss()
    }

    private static let shortDate: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        return f
    }()
}

/// Inline mini-timeline of the last handful of events for a given batch.
/// Lives in `BatchDetailView` so the full history sheet stays elsewhere.
private struct MiniBatchHistory: View {
    @Environment(AppState.self) private var appState
    let batchID: UUID

    @State private var entries: [StockEvent] = []
    @State private var isLoading = true

    var body: some View {
        Group {
            if isLoading {
                ProgressView()
            } else if entries.isEmpty {
                Text("No events recorded.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(entries) { event in
                    StockEventRowView(event: event)
                }
            }
        }
        .task { await load() }
    }

    private func load() async {
        isLoading = true
        let page = try? await appState.api.listBatchEvents(id: batchID, limit: 10)
        entries = page?.items ?? []
        isLoading = false
    }
}
