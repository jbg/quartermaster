import SwiftUI

struct ProductDetailView: View {
    enum Action {
        case updated(Product)
        case refreshed(Product)
        case deleted
        case restored(Product)
    }

    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    let product: Product
    var onChange: (Action) -> Void

    @State private var name: String
    @State private var brand: String
    @State private var family: ProductFamily
    @State private var preferredUnit: String
    @State private var imageURLText: String
    @State private var isSubmitting = false
    @State private var errorMessage: String?
    @State private var confirmDelete = false

    init(product: Product, onChange: @escaping (Action) -> Void) {
        self.product = product
        self.onChange = onChange
        _name = State(initialValue: product.name)
        _brand = State(initialValue: product.brand ?? "")
        _family = State(initialValue: product.family)
        _preferredUnit = State(initialValue: product.preferredUnit)
        _imageURLText = State(initialValue: product.imageURL?.absoluteString ?? "")
    }

    var body: some View {
        NavigationStack {
            Form {
                if product.isOFF {
                    offBody
                } else {
                    manualBody
                }

                if let msg = errorMessage {
                    Section {
                        Text(msg).foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle("Product")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(product.isManual && !product.isDeleted ? "Cancel" : "Done") { dismiss() }
                }
                if product.isManual && !product.isDeleted {
                    ToolbarItem(placement: .confirmationAction) {
                        Button { Task { await save() } } label: {
                            if isSubmitting { ProgressView() } else { Text("Save").fontWeight(.semibold) }
                        }
                        .disabled(!canSave || isSubmitting)
                    }
                }
            }
            .confirmationDialog(
                "Delete this product?",
                isPresented: $confirmDelete,
                titleVisibility: .visible,
            ) {
                Button("Delete", role: .destructive) {
                    Task { await delete() }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Its batches must already be empty.")
            }
        }
    }

    // MARK: - Manual form

    @ViewBuilder
    private var manualBody: some View {
        if product.isDeleted {
            deletedManualBody
        } else {
            editableManualBody
        }
    }

    @ViewBuilder
    private var editableManualBody: some View {
        Section("Product") {
            TextField("Name", text: $name)
            TextField("Brand (optional)", text: $brand)
        }
        Section("Unit family") {
            Picker("Family", selection: $family) {
                ForEach(ProductFamily.allCases, id: \.self) { f in
                    Text(f.displayName).tag(f)
                }
            }
            Picker("Preferred unit", selection: $preferredUnit) {
                ForEach(appState.unitsFor(family: family), id: \.code) { u in
                    Text(u.code).tag(u.code)
                }
            }
        }
        Section {
            TextField("Image URL (optional)", text: $imageURLText)
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
                .autocorrectionDisabled()
        } footer: {
            Text("Used as the thumbnail in inventory lists.")
        }
        Section {
            Button(role: .destructive) {
                confirmDelete = true
            } label: {
                Label("Delete product", systemImage: "trash")
            }
        } footer: {
            Text("Products with active stock can't be deleted. Consume or remove the batches first.")
        }
        .onChange(of: family) { _, newFamily in
            if !appState.unitsFor(family: newFamily).contains(where: { $0.code == preferredUnit }) {
                preferredUnit = newFamily.baseUnit
            }
        }
    }

    @ViewBuilder
    private var deletedManualBody: some View {
        Section("Deleted product") {
            LabeledContent("Name", value: product.name)
            if let brand = product.brand, !brand.isEmpty {
                LabeledContent("Brand", value: brand)
            }
            LabeledContent("Family", value: product.family.displayName)
            if let deletedAt = product.deletedAt {
                LabeledContent("Deleted at", value: deletedAt)
            }
        }
        Section {
            Button {
                Task { await restore() }
            } label: {
                if isSubmitting {
                    ProgressView()
                } else {
                    Label("Restore product", systemImage: "arrow.uturn.backward")
                }
            }
            .disabled(isSubmitting)
        } footer: {
            Text("The product becomes searchable and usable again. Any old depleted batches that referenced it already retained their history.")
        }
    }

    // MARK: - OFF form

    @ViewBuilder
    private var offBody: some View {
        Section("Product") {
            LabeledContent("Name", value: product.name)
            if let brand = product.brand, !brand.isEmpty {
                LabeledContent("Brand", value: brand)
            }
            LabeledContent("Family", value: product.family.displayName)
            if let barcode = product.barcode {
                LabeledContent("Barcode") {
                    Text(barcode).monospaced()
                }
            }
        }
        Section {
            Button {
                Task { await refreshFromOFF() }
            } label: {
                if isSubmitting {
                    ProgressView()
                } else {
                    Label("Refresh from Open Food Facts", systemImage: "arrow.clockwise")
                }
            }
            .disabled(isSubmitting)
        } footer: {
            Text("This product is sourced from Open Food Facts. Details are managed there — tap refresh to pull the latest values.")
        }
    }

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
    }

    // MARK: - Actions

    private func save() async {
        isSubmitting = true
        errorMessage = nil
        var request = UpdateProductRequest()
        let trimmedName = name.trimmingCharacters(in: .whitespaces)
        if trimmedName != product.name {
            request.name = trimmedName
        }
        let trimmedBrand = brand.trimmingCharacters(in: .whitespaces)
        switch (product.brand, trimmedBrand) {
        case (_, "") where product.brand != nil:
            request.clearBrand = true
        case let (existing, new) where existing != new && !new.isEmpty:
            request.brand = new
        default:
            break
        }
        if family != product.family {
            request.family = family
        }
        if preferredUnit != product.preferredUnit {
            request.preferredUnit = preferredUnit
        }
        let trimmedImage = imageURLText.trimmingCharacters(in: .whitespaces)
        let existingImage = product.imageURL?.absoluteString
        switch (existingImage, trimmedImage) {
        case (_, "") where existingImage != nil:
            request.clearImageURL = true
        case let (existing, new) where existing != new && !new.isEmpty:
            request.imageURL = new
        default:
            break
        }

        do {
            let updated = try await appState.api.updateProduct(id: product.id, request: request)
            onChange(.updated(updated))
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }

    private func delete() async {
        isSubmitting = true
        errorMessage = nil
        do {
            try await appState.api.deleteProduct(id: product.id)
            onChange(.deleted)
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }

    private func refreshFromOFF() async {
        isSubmitting = true
        errorMessage = nil
        do {
            let refreshed = try await appState.api.refreshProduct(id: product.id)
            onChange(.refreshed(refreshed))
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }

    private func restore() async {
        isSubmitting = true
        errorMessage = nil
        do {
            let restored = try await appState.api.restoreProduct(id: product.id)
            onChange(.restored(restored))
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }
}
