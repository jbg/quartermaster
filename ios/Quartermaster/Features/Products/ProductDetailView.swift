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
  @State private var imageURLValid: Bool = true
  @State private var hasPackageSize: Bool
  @State private var packageQuantity: String
  @State private var packageUnit: String
  @State private var maxOpenDaysText: String
  @State private var isSubmitting = false
  @State private var errorMessage: String?
  @State private var confirmDelete = false
  @State private var offContributionPreview: OffContributionPreviewResponse?

  init(product: Product, onChange: @escaping (Action) -> Void) {
    self.product = product
    self.onChange = onChange
    _name = State(initialValue: product.name)
    _brand = State(initialValue: product.brand ?? "")
    _family = State(initialValue: product.family)
    _preferredUnit = State(initialValue: product.preferredUnit)
    _imageURLText = State(initialValue: product.imageURL?.absoluteString ?? "")
    _hasPackageSize = State(
      initialValue: product.packageQuantity != nil && product.packageUnit != nil)
    _packageQuantity = State(initialValue: product.packageQuantity ?? "")
    _packageUnit = State(initialValue: product.packageUnit ?? product.family.baseUnit)
    _maxOpenDaysText = State(initialValue: product.maxOpenDays.map(String.init) ?? "")
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
            Text(msg).foregroundStyle(Color.quartermasterError)
          }
        }
      }
      .navigationTitle("Product")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button((product.isManual || product.isOFF) && !product.isDeleted ? "Cancel" : "Done") {
            dismiss()
          }
        }
        if (product.isManual || product.isOFF) && !product.isDeleted {
          ToolbarItem(placement: .confirmationAction) {
            Button {
              Task { await save() }
            } label: {
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
      .task {
        if product.isOFF {
          offContributionPreview = try? await appState.api.offContributionPreview(
            productID: product.id)
        }
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
    unitFamilySection
    packageSizeSection
    Section {
      ValidatedURLField(
        title: "Image URL (optional)",
        text: $imageURLText,
        isValid: $imageURLValid,
      )
    } footer: {
      Text("Used as the thumbnail in inventory lists.")
    }
    Section {
      TextField("Maximum open days (optional)", text: $maxOpenDaysText)
        .keyboardType(.numberPad)
    } footer: {
      Text("Used to date leftovers when an opened package is stored.")
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
      Text(
        "The product becomes searchable and usable again. Any old depleted batches that referenced it already retained their history."
      )
    }
  }

  // MARK: - OFF form

  @ViewBuilder
  private var offBody: some View {
    Section("Product") {
      TextField("Name", text: $name)
      TextField("Brand (optional)", text: $brand)
      if let barcode = product.barcode {
        LabeledContent("Barcode") {
          Text(barcode).monospaced()
        }
      }
    }
    unitFamilySection
    packageSizeSection
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
      Text(
        "This product is sourced from Open Food Facts. Package size corrections are kept locally; tap refresh to pull the latest catalogue values."
      )
    }
    if offContributionPreview?.credentialsPresent == true,
      !(offContributionPreview?.changedFields.isEmpty ?? true)
    {
      Section {
        Button {
          Task { await contributeToOFF() }
        } label: {
          if isSubmitting {
            ProgressView()
          } else {
            Label("Contribute to Open Food Facts", systemImage: "square.and.arrow.up")
          }
        }
        .disabled(isSubmitting)
      } footer: {
        Text("Submits your local corrections using your saved Open Food Facts account.")
      }
    }
  }

  private var packageSizeSection: some View {
    Section {
      Toggle("Comes in packages", isOn: $hasPackageSize.animation())
      if hasPackageSize {
        DecimalField(title: "Amount per package", text: $packageQuantity)
        Picker("Package unit", selection: $packageUnit) {
          ForEach(appState.unitsFor(family: family), id: \.code) { u in
            Text(u.code).tag(u.code)
          }
        }
      }
    } header: {
      Text("Package size")
    } footer: {
      Text("Used by the scan flow to add one inventory batch per package.")
    }
  }

  private var canSave: Bool {
    if product.isOFF {
      return !name.trimmingCharacters(in: .whitespaces).isEmpty && packageSizeIsValid
        && (name != product.name || brand != (product.brand ?? "") || family != product.family
          || preferredUnit != product.preferredUnit || packageSizeChanged)
    }
    return !name.trimmingCharacters(in: .whitespaces).isEmpty && imageURLValid
      && packageSizeIsValid && parsedMaxOpenDays != 0
  }

  private var packageSizeIsValid: Bool {
    parsedPackageQuantity != 0
      && (!hasPackageSize || appState.unitsFor(family: family).contains { $0.code == packageUnit })
  }

  private var parsedPackageQuantity: Decimal? {
    guard hasPackageSize else { return nil }
    let trimmed = packageQuantity.trimmingCharacters(in: .whitespaces)
    guard let value = Decimal(string: trimmed), value > 0 else { return 0 }
    return value
  }

  private var packageSizeChanged: Bool {
    if hasPackageSize, let parsedPackageQuantity {
      let quantity = NSDecimalNumber(decimal: parsedPackageQuantity).stringValue
      return quantity != (product.packageQuantity ?? "")
        || packageUnit != (product.packageUnit ?? product.family.baseUnit)
    }
    return product.packageQuantity != nil || product.packageUnit != nil
  }

  private var unitFamilySection: some View {
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
    .onChange(of: family) { _, newFamily in
      if !appState.unitsFor(family: newFamily).contains(where: { $0.code == preferredUnit }) {
        preferredUnit = newFamily.baseUnit
      }
      if !appState.unitsFor(family: newFamily).contains(where: { $0.code == packageUnit }) {
        packageUnit = newFamily.baseUnit
      }
    }
  }

  private var parsedMaxOpenDays: Int64? {
    let trimmed = maxOpenDaysText.trimmingCharacters(in: .whitespaces)
    if trimmed.isEmpty { return nil }
    guard let value = Int64(trimmed), value > 0 else { return 0 }
    return value
  }

  // MARK: - Actions

  private func save() async {
    isSubmitting = true
    errorMessage = nil
    var request = UpdateProductRequest()
    let trimmedName = name.trimmingCharacters(in: .whitespaces)
    if trimmedName != product.name {
      request.append(jsonPatchReplace("/name", trimmedName))
    }
    let trimmedBrand = brand.trimmingCharacters(in: .whitespaces)
    switch (product.brand, trimmedBrand) {
    case (_, "") where product.brand != nil:
      request.append(jsonPatchRemove("/brand"))
    case (let existing, let new) where existing != new && !new.isEmpty:
      request.append(jsonPatchReplace("/brand", new))
    default:
      break
    }
    if family != product.family {
      request.append(jsonPatchReplace("/family", family.rawValue))
    }
    if preferredUnit != product.preferredUnit {
      request.append(jsonPatchReplace("/preferred_unit", preferredUnit))
    }
    let trimmedImage = imageURLText.trimmingCharacters(in: .whitespaces)
    let existingImage = product.imageURL?.absoluteString
    switch (existingImage, trimmedImage) {
    case (_, "") where existingImage != nil:
      request.append(jsonPatchRemove("/image_url"))
    case (let existing, let new) where existing != new && !new.isEmpty:
      request.append(jsonPatchReplace("/image_url", new))
    default:
      break
    }
    switch (product.maxOpenDays, parsedMaxOpenDays) {
    case (_?, nil):
      request.append(jsonPatchRemove("/max_open_days"))
    case (let existing, let new?) where existing != new:
      request.append(jsonPatchReplace("/max_open_days", new))
    default:
      break
    }
    if hasPackageSize, let parsedPackageQuantity {
      let quantity = NSDecimalNumber(decimal: parsedPackageQuantity).stringValue
      if quantity != (product.packageQuantity ?? "") {
        request.append(jsonPatchReplace("/package_quantity", quantity))
      }
      if product.packageUnit == nil || packageUnit != product.packageUnit {
        request.append(jsonPatchReplace("/package_unit", packageUnit))
      }
    } else if !hasPackageSize && (product.packageQuantity != nil || product.packageUnit != nil) {
      request.append(jsonPatchRemove("/package_quantity"))
      request.append(jsonPatchRemove("/package_unit"))
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

  private func contributeToOFF() async {
    isSubmitting = true
    errorMessage = nil
    do {
      let response = try await appState.api.contributeProductToOFF(id: product.id)
      onChange(.updated(response.product))
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
