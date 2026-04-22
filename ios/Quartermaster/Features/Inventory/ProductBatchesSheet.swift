import SwiftUI

struct ProductBatchesSheet: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    let product: Product
    let location: Location
    let allLocations: [Location]
    @State var batches: [StockBatch]
    var highlightBatchID: String? = nil
    var onMutated: () async -> Void

    @State private var editing: StockBatch?
    @State private var consumeTarget: StockBatch?
    @State private var showProductDetails = false
    @State private var showBatchHistory: StockBatch?
    @State private var errorMessage: String?
    /// Batch currently showing the "you came from here" flash. Cleared by a
    /// background task after ~1.5 s so the pulse feels transient.
    @State private var flashingBatchID: String?

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                List {
                    ForEach(batches) { batch in
                        BatchRow(batch: batch)
                            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                Button(role: .destructive) {
                                    Task { await delete(batch) }
                                } label: {
                                    Label("Delete", systemImage: "trash")
                                }
                            }
                            .swipeActions(edge: .leading) {
                                Button {
                                    editing = batch
                                } label: {
                                    Label("Edit", systemImage: "pencil")
                                }
                                .tint(.blue)
                            }
                            .contentShape(Rectangle())
                            .onTapGesture { editing = batch }
                            .listRowBackground(
                                flashingBatchID == batch.id
                                    ? Color.accentColor.opacity(0.18)
                                    : Color.clear,
                            )
                            .animation(.easeOut(duration: 0.4), value: flashingBatchID)
                            .id(batch.id)
                    }
                    Section {
                        Button {
                            consumeTarget = batches.first
                        } label: {
                            Label("Consume", systemImage: "fork.knife")
                        }
                        .disabled(batches.isEmpty)
                    }
                }
                .task(id: highlightBatchID) { await flashHighlight(proxy: proxy) }
            }
            .navigationTitle(product.displayTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Menu {
                        Button {
                            showProductDetails = true
                        } label: {
                            Label("Product details", systemImage: "info.circle")
                        }
                        if let first = batches.first {
                            Button {
                                showBatchHistory = first
                            } label: {
                                Label("Batch history", systemImage: "clock.arrow.circlepath")
                            }
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(item: $editing) { batch in
                EditBatchForm(batch: batch, product: product, locations: allLocations) { updated in
                    if let idx = batches.firstIndex(where: { $0.id == updated.id }) {
                        batches[idx] = updated
                    }
                    Task { await onMutated() }
                }
            }
            .sheet(item: $consumeTarget) { _ in
                ConsumeForm(product: product, location: location) {
                    await onMutated()
                    if let refreshed = try? await appState.api.listStock(locationID: location.id, productID: product.id) {
                        batches = refreshed
                    }
                    if batches.isEmpty { dismiss() }
                }
            }
            .sheet(isPresented: $showProductDetails) {
                ProductDetailView(product: product) { action in
                    Task {
                        await onMutated()
                        switch action {
                        case .deleted:
                            dismiss()
                        case .updated, .refreshed, .restored:
                            break
                        }
                    }
                }
            }
            .sheet(item: $showBatchHistory) { batch in
                NavigationStack {
                    StockHistoryView(scope: .batch(batch.id)) {
                        await onMutated()
                        if let refreshed = try? await appState.api.listStock(
                            locationID: location.id,
                            productID: product.id,
                        ) {
                            batches = refreshed
                        }
                    }
                }
            }
            .alert("Couldn't update stock", isPresented: Binding(
                get: { errorMessage != nil },
                set: { if !$0 { errorMessage = nil } }
            )) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(errorMessage ?? "")
            }
        }
    }

    private func flashHighlight(proxy: ScrollViewProxy) async {
        guard let target = highlightBatchID,
              batches.contains(where: { $0.id == target })
        else { return }
        withAnimation {
            proxy.scrollTo(target, anchor: .center)
            flashingBatchID = target
        }
        try? await Task.sleep(for: .milliseconds(1400))
        withAnimation { flashingBatchID = nil }
    }

    private func delete(_ batch: StockBatch) async {
        do {
            try await appState.api.deleteStock(id: batch.id)
            batches.removeAll { $0.id == batch.id }
            await onMutated()
            if batches.isEmpty { dismiss() }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct BatchRow: View {
    let batch: StockBatch

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("\(batch.quantity) \(batch.unit)")
                    .font(.body.weight(.medium))
                if let note = batch.note, !note.isEmpty {
                    Text(note).font(.caption).foregroundStyle(.secondary)
                }
                if let opened = batch.openedOnDate {
                    Label("Opened \(Self.relativeDate(opened))", systemImage: "seal")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
            ExpiryBadge(expiresOn: batch.expiresOn)
        }
        .padding(.vertical, 2)
    }

    private static func relativeDate(_ d: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .none
        return formatter.string(from: d)
    }
}

private struct EditBatchForm: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    let batch: StockBatch
    let product: Product
    let locations: [Location]
    var onUpdated: (StockBatch) -> Void

    @State private var quantity: String
    @State private var locationID: String
    @State private var hasExpiry: Bool
    @State private var expiry: Date
    @State private var hadExpiryOriginally: Bool
    @State private var hasOpened: Bool
    @State private var opened: Date
    @State private var hadOpenedOriginally: Bool
    @State private var note: String
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    init(batch: StockBatch, product: Product, locations: [Location], onUpdated: @escaping (StockBatch) -> Void) {
        self.batch = batch
        self.product = product
        self.locations = locations
        self.onUpdated = onUpdated
        _quantity = State(initialValue: batch.quantity)
        _locationID = State(initialValue: batch.locationID)
        let originalExpiry = batch.expiresOn != nil
        _hasExpiry = State(initialValue: originalExpiry)
        _hadExpiryOriginally = State(initialValue: originalExpiry)
        _expiry = State(initialValue: batch.expiresOnDate ?? Calendar.current.date(byAdding: .day, value: 30, to: .now) ?? .now)
        let originalOpened = batch.openedOn != nil
        _hasOpened = State(initialValue: originalOpened)
        _hadOpenedOriginally = State(initialValue: originalOpened)
        _opened = State(initialValue: batch.openedOnDate ?? .now)
        _note = State(initialValue: batch.note ?? "")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    DecimalField(title: "Amount", text: $quantity)
                    LabeledContent("Unit", value: batch.unit)
                        .foregroundStyle(.secondary)
                } header: {
                    Text("Quantity")
                } footer: {
                    Text("The unit is fixed when the batch is added. Change it by deleting the batch and re-adding.")
                }
                Section("Location") {
                    Picker("Location", selection: $locationID) {
                        ForEach(locations) { loc in
                            Text(loc.name).tag(loc.id)
                        }
                    }
                }
                Section {
                    Toggle("Set expiry date", isOn: $hasExpiry.animation())
                    if hasExpiry {
                        DatePicker("Expires", selection: $expiry, displayedComponents: .date)
                    }
                    if hadExpiryOriginally && !hasExpiry {
                        Text("Saving will remove the expiry date from this batch.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                Section {
                    Toggle("Mark as opened", isOn: $hasOpened.animation())
                    if hasOpened {
                        DatePicker("Opened on", selection: $opened, displayedComponents: .date)
                    }
                }
                Section("Note") {
                    TextField("Optional", text: $note, axis: .vertical)
                }
                if let msg = errorMessage {
                    Section { Text(msg).foregroundStyle(.red) }
                }
            }
            .navigationTitle("Edit batch")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button { Task { await submit() } } label: {
                        if isSubmitting { ProgressView() } else { Text("Save").fontWeight(.semibold) }
                    }
                    .disabled(isSubmitting)
                }
            }
        }
    }

    private func submit() async {
        isSubmitting = true
        errorMessage = nil
        var request = UpdateStockRequest()
        if quantity != batch.quantity {
            request.quantity = quantity
        }
        if locationID != batch.locationID {
            request.locationID = locationID
        }
        if hasExpiry {
            let s = StockBatch.yyyymmdd.string(from: expiry)
            if s != batch.expiresOn {
                request.expiresOn = s
            }
        } else if hadExpiryOriginally {
            request.clearExpiresOn = true
        }
        if hasOpened {
            let s = StockBatch.yyyymmdd.string(from: opened)
            if s != batch.openedOn {
                request.openedOn = s
            }
        } else if hadOpenedOriginally {
            request.clearOpenedOn = true
        }
        let trimmedNote = note.trimmingCharacters(in: .whitespaces)
        if trimmedNote.isEmpty {
            if batch.note != nil {
                request.clearNote = true
            }
        } else if trimmedNote != batch.note {
            request.note = trimmedNote
        }

        do {
            let updated = try await appState.api.updateStock(id: batch.id, request: request)
            onUpdated(updated)
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }
}

private struct ConsumeForm: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    let product: Product
    let location: Location
    var onConsumed: () async -> Void

    @State private var quantity: String = ""
    @State private var unitCode: String
    @State private var isSubmitting = false
    @State private var errorMessage: String?
    @State private var successMessage: String?

    init(product: Product, location: Location, onConsumed: @escaping () async -> Void) {
        self.product = product
        self.location = location
        self.onConsumed = onConsumed
        _unitCode = State(initialValue: product.preferredUnit)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("How much did you use?") {
                    DecimalField(title: "Amount", text: $quantity)
                    Picker("Unit", selection: $unitCode) {
                        ForEach(appState.unitsFor(family: product.family), id: \.code) { u in
                            Text(u.code).tag(u.code)
                        }
                    }
                }
                Section {
                    Text("We'll take from the batch that expires soonest first.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                if let msg = errorMessage {
                    Section { Text(msg).foregroundStyle(.red) }
                }
            }
            .navigationTitle("Consume \(product.name)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button { Task { await submit() } } label: {
                        if isSubmitting { ProgressView() } else { Text("Consume").fontWeight(.semibold) }
                    }
                    .disabled(!canSubmit || isSubmitting)
                }
            }
            .alert("Consumed", isPresented: Binding(
                get: { successMessage != nil },
                set: { if !$0 { successMessage = nil } }
            )) {
                Button("OK") {
                    Task {
                        await onConsumed()
                        dismiss()
                    }
                }
            } message: {
                Text(successMessage ?? "")
            }
        }
    }

    private var canSubmit: Bool {
        guard !quantity.isEmpty, let value = Decimal(string: quantity), value > 0 else { return false }
        return true
    }

    private func submit() async {
        isSubmitting = true
        errorMessage = nil
        let request = ConsumeRequest(
            locationId: location.id,
            productId: product.id,
            quantity: quantity,
            unit: unitCode,
        )
        do {
            let response = try await appState.api.consumeStock(request)
            successMessage = buildSuccessMessage(response: response)
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }

    private func buildSuccessMessage(response: ConsumeResponse) -> String {
        let total = response.consumed.reduce(Decimal.zero) { partial, c in
            partial + (Decimal(string: c.quantityInRequestedUnit) ?? .zero)
        }
        let totalLabel = "\(Self.format(total)) \(unitCode)"
        let count = response.consumed.count
        if count <= 1 {
            return "Consumed \(totalLabel)."
        }
        return "Consumed \(totalLabel) across \(count) batches."
    }

    private static func format(_ d: Decimal) -> String {
        var copy = d
        var rounded = Decimal()
        NSDecimalRound(&rounded, &copy, 3, .plain)
        return NSDecimalNumber(decimal: rounded).stringValue
    }
}
