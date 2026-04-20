import SwiftUI

struct ProductBatchesSheet: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    let product: Product
    let location: Location
    @State var batches: [StockBatch]
    var onMutated: () async -> Void

    @State private var editing: StockBatch?
    @State private var consumeTarget: StockBatch?
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
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
            .navigationTitle(product.displayTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(item: $editing) { batch in
                EditBatchForm(batch: batch, product: product, locations: [location]) { updated in
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
            }
            Spacer()
            ExpiryBadge(expiresOn: batch.expiresOnDate)
        }
        .padding(.vertical, 2)
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
    @State private var unitCode: String
    @State private var locationID: UUID
    @State private var hasExpiry: Bool
    @State private var expiry: Date
    @State private var note: String
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    init(batch: StockBatch, product: Product, locations: [Location], onUpdated: @escaping (StockBatch) -> Void) {
        self.batch = batch
        self.product = product
        self.locations = locations
        self.onUpdated = onUpdated
        _quantity = State(initialValue: batch.quantity)
        _unitCode = State(initialValue: batch.unit)
        _locationID = State(initialValue: batch.locationID)
        _hasExpiry = State(initialValue: batch.expiresOn != nil)
        _expiry = State(initialValue: batch.expiresOnDate ?? Calendar.current.date(byAdding: .day, value: 30, to: .now) ?? .now)
        _note = State(initialValue: batch.note ?? "")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Quantity") {
                    TextField("Amount", text: $quantity).keyboardType(.decimalPad)
                    Picker("Unit", selection: $unitCode) {
                        ForEach(appState.unitsFor(family: product.family), id: \.code) { u in
                            Text(u.code).tag(u.code)
                        }
                    }
                }
                Section {
                    Toggle("Set expiry date", isOn: $hasExpiry.animation())
                    if hasExpiry {
                        DatePicker("Expires", selection: $expiry, displayedComponents: .date)
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
        let request = UpdateStockRequest(
            quantity: quantity,
            unit: unitCode,
            locationID: locationID != batch.locationID ? locationID : nil,
            expiresOn: hasExpiry ? Self.iso.string(from: expiry) : nil,
            note: note.trimmingCharacters(in: .whitespaces).isEmpty ? nil : note,
        )
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

    private static let iso: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = .init(identifier: "UTC")
        f.locale = .init(identifier: "en_US_POSIX")
        return f
    }()
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
                    TextField("Amount", text: $quantity).keyboardType(.decimalPad)
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
            productID: product.id,
            locationID: location.id,
            quantity: quantity,
            unit: unitCode,
        )
        do {
            _ = try await appState.api.consumeStock(request)
            await onConsumed()
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }
}
