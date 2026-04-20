import SwiftUI

struct ManualProductForm: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    var prefillBarcode: String?
    var onCreated: (Product) -> Void

    @State private var name: String = ""
    @State private var brand: String = ""
    @State private var family: ProductFamily = .count
    @State private var preferredUnit: String = ProductFamily.count.baseUnit
    @State private var barcode: String = ""
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
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
                Section("Barcode") {
                    TextField("Optional", text: $barcode)
                        .keyboardType(.numberPad)
                        .textInputAutocapitalization(.never)
                }
                if let msg = errorMessage {
                    Section {
                        Text(msg).foregroundStyle(.red)
                    }
                }
                Section {
                    Text("Products you create manually are only visible to your household. OpenFoodFacts-sourced products are shared.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("New product")
            .navigationBarTitleDisplayMode(.inline)
            .onAppear {
                if let prefillBarcode, barcode.isEmpty {
                    barcode = prefillBarcode
                }
            }
            .onChange(of: family) { _, newFamily in
                preferredUnit = newFamily.baseUnit
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button { Task { await submit() } } label: {
                        if isSubmitting { ProgressView() } else { Text("Create").fontWeight(.semibold) }
                    }
                    .disabled(!canSubmit || isSubmitting)
                }
            }
        }
    }

    private var canSubmit: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private func submit() async {
        isSubmitting = true
        errorMessage = nil
        let cleanBrand = brand.trimmingCharacters(in: .whitespaces)
        let cleanBarcode = barcode.trimmingCharacters(in: .whitespaces)
        let request = CreateProductRequest(
            name: name.trimmingCharacters(in: .whitespaces),
            brand: cleanBrand.isEmpty ? nil : cleanBrand,
            family: family,
            preferredUnit: preferredUnit.isEmpty ? nil : preferredUnit,
            barcode: cleanBarcode.isEmpty ? nil : cleanBarcode,
        )
        do {
            let created = try await appState.api.createProduct(request)
            onCreated(created)
            dismiss()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isSubmitting = false
    }
}
