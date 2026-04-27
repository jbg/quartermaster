import SwiftUI

struct ManualProductForm: View {
  @Environment(AppState.self) private var appState
  @Environment(\.dismiss) private var dismiss

  var prefillBarcode: String?
  var onCreated: (Product) -> Void

  @State private var name: String = ""
  @State private var brand: String = ""
  @State private var family: ProductFamily = .mass
  @State private var preferredUnit: String = ProductFamily.mass.baseUnit
  @State private var barcode: String = ""
  @State private var imageURLText: String = ""
  @State private var imageURLValid: Bool = true
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
        Section {
          ValidatedURLField(
            title: "Image URL (optional)",
            text: $imageURLText,
            isValid: $imageURLValid,
          )
        } footer: {
          Text("Used as the thumbnail in inventory lists.")
        }
        Section("Barcode") {
          TextField("Optional", text: $barcode)
            .keyboardType(.numberPad)
            .textInputAutocapitalization(.never)
        }
        if let msg = errorMessage {
          Section {
            Text(msg).foregroundStyle(Color.quartermasterError)
          }
        }
        Section {
          Text(
            "Products you create manually are only visible to your household. OpenFoodFacts-sourced products are shared."
          )
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
          Button {
            Task { await submit() }
          } label: {
            if isSubmitting { ProgressView() } else { Text("Create").fontWeight(.semibold) }
          }
          .disabled(!canSubmit || isSubmitting)
        }
      }
    }
  }

  private var canSubmit: Bool {
    !name.trimmingCharacters(in: .whitespaces).isEmpty && imageURLValid
  }

  private func submit() async {
    isSubmitting = true
    errorMessage = nil
    let cleanBrand = brand.trimmingCharacters(in: .whitespaces)
    let cleanBarcode = barcode.trimmingCharacters(in: .whitespaces)
    let cleanImageURL = imageURLText.trimmingCharacters(in: .whitespaces)
    let request = CreateProductRequest(
      barcode: cleanBarcode.isEmpty ? nil : cleanBarcode,
      brand: cleanBrand.isEmpty ? nil : cleanBrand,
      family: family,
      imageUrl: cleanImageURL.isEmpty ? nil : cleanImageURL,
      name: name.trimmingCharacters(in: .whitespaces),
      preferredUnit: preferredUnit.isEmpty ? nil : preferredUnit,
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
