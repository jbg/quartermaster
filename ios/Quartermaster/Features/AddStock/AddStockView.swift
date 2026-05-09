import SwiftUI

struct AddStockView: View {
  @Environment(AppState.self) private var appState
  @Environment(\.dismiss) private var dismiss

  let product: Product
  var onAdded: ((StockBatch) -> Void)?

  private enum QuantityEntryMode: String, CaseIterable, Identifiable {
    case package
    case exact

    var id: String { rawValue }
  }

  @State private var entryMode: QuantityEntryMode = .exact
  @State private var packageCount: String = ""
  @State private var quantity: String = ""
  @State private var unitCode: String = ""
  @State private var selectedLocationID: String?
  @State private var selectedStorageVesselID: String?
  @State private var quantityIncludesStorageVessel = false
  @State private var hasExpiry: Bool = false
  @State private var expiry: Date =
    Calendar.current.date(byAdding: .day, value: 30, to: .now) ?? .now
  @State private var showExpiryScanner = false
  @State private var hasProduced: Bool = false
  @State private var produced: Date = .now
  @State private var hasOpened: Bool = false
  @State private var opened: Date = .now
  @State private var note: String = ""

  @State private var locations: [Location] = []
  @State private var storageVessels: [StorageVessel] = []
  @State private var isSubmitting = false
  @State private var errorMessage: String?

  var body: some View {
    NavigationStack {
      Form {
        Section("Product") {
          productHeader
        }
        Section {
          if productPackageSize != nil {
            Picker("Entry", selection: $entryMode) {
              Text("Packages").tag(QuantityEntryMode.package)
              Text("Exact amount").tag(QuantityEntryMode.exact)
            }
            .pickerStyle(.segmented)
          }
          if entryMode == .package, let packageSize = productPackageSize {
            DecimalField(title: "Packages", text: $packageCount)
            LabeledContent("Each", value: "\(packageSize.quantity) \(packageSize.unit)")
          } else {
            DecimalField(title: "Amount", text: $quantity)
            Picker("Unit", selection: $unitCode) {
              ForEach(unitOptions, id: \.code) { u in
                Text(u.code).tag(u.code)
              }
            }
          }
        } header: {
          Text("Quantity")
        } footer: {
          if entryMode == .package, productPackageSize != nil {
            Text("Quartermaster adds one inventory batch per package.")
          }
        }
        Section("Location") {
          Picker("Location", selection: $selectedLocationID) {
            ForEach(locations) { loc in
              Text(loc.name).tag(Optional(loc.id))
            }
          }
        }
        Section {
          Picker("Vessel", selection: $selectedStorageVesselID) {
            Text("None").tag(Optional<String>.none)
            ForEach(storageVessels) { vessel in
              Text("\(vessel.name) (\(vessel.displayTare))").tag(Optional(vessel.id))
            }
          }
          if grossVesselWeightEligible {
            Toggle("Amount includes vessel", isOn: $quantityIncludesStorageVessel)
          }
        } header: {
          Text("Storage Vessel")
        } footer: {
          if grossVesselWeightEligible, let vessel = selectedStorageVessel {
            Text("Quartermaster will subtract \(vessel.displayTare) before saving this batch.")
          } else if selectedStorageVesselID != nil && productPackageSize != nil {
            Text(
              "Vessel subtraction is only available for exact free-weight entries, not package entries."
            )
          }
        }
        Section {
          Toggle("Set prepared date", isOn: $hasProduced.animation())
          if hasProduced {
            DatePicker("Prepared on", selection: $produced, displayedComponents: .date)
          }
        } footer: {
          Text("Useful for leftovers, meal prep, and homemade batches.")
        }
        Section {
          Toggle("Set expiry date", isOn: $hasExpiry.animation())
          if hasExpiry {
            DatePicker("Expires", selection: $expiry, displayedComponents: .date)
          }
          Button {
            showExpiryScanner = true
          } label: {
            Label("Scan expiry date", systemImage: "text.viewfinder")
          }
        }
        Section {
          Toggle("Mark as opened", isOn: $hasOpened.animation())
          if hasOpened {
            DatePicker("Opened on", selection: $opened, displayedComponents: .date)
          }
        } footer: {
          Text("For items with a 'best within N days once opened' rule.")
        }
        Section("Note") {
          TextField("Optional", text: $note, axis: .vertical)
        }
        if let msg = errorMessage {
          Section {
            Text(msg).foregroundStyle(Color.quartermasterError)
          }
        }
      }
      .sheet(isPresented: $showExpiryScanner) {
        ExpiryDateScannerView { candidate in
          expiry = candidate.date
          hasExpiry = true
        }
      }
      .navigationTitle("Add to inventory")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Cancel") { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
          Button {
            Task { await submit() }
          } label: {
            if isSubmitting { ProgressView() } else { Text("Add").fontWeight(.semibold) }
          }
          .disabled(!canSubmit || isSubmitting)
        }
      }
      .task { await loadInitial() }
      .onChange(of: grossVesselWeightEligible) { _, eligible in
        if !eligible {
          quantityIncludesStorageVessel = false
        }
      }
    }
  }

  private var productHeader: some View {
    HStack(spacing: 12) {
      if let url = product.imageURL {
        AsyncImage(url: url) { phase in
          switch phase {
          case .success(let image):
            image.resizable().scaledToFit()
          default:
            Color.secondary.opacity(0.1)
          }
        }
        .frame(width: 48, height: 48)
        .clipShape(RoundedRectangle(cornerRadius: 8))
      }
      VStack(alignment: .leading, spacing: 2) {
        Text(product.displayTitle).font(.headline)
        HStack(spacing: 4) {
          Image(systemName: familyIcon)
          Text(product.family.displayName)
          if let barcode = product.barcode {
            Text("·")
            Text(barcode).monospaced()
          }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
      }
    }
  }

  private var unitOptions: [Unit] {
    appState.unitsFor(family: product.family)
  }

  private var productPackageSize: (quantity: String, unit: String)? {
    guard
      let quantity = product.packageQuantity,
      let unit = product.packageUnit,
      Decimal(string: quantity).map({ $0 > 0 }) == true
    else { return nil }
    return (quantity, unit)
  }

  private var selectedStorageVessel: StorageVessel? {
    guard let selectedStorageVesselID else { return nil }
    return storageVessels.first { $0.id == selectedStorageVesselID }
  }

  private var grossVesselWeightEligible: Bool {
    entryMode == .exact && product.family == .mass && selectedStorageVessel != nil
      && productPackageSize == nil
  }

  private var canSubmit: Bool {
    guard selectedLocationID != nil else { return false }
    if entryMode == .package {
      guard productPackageSize != nil else { return false }
      return wholePackageCount() != nil
    } else {
      guard !quantity.isEmpty, !unitCode.isEmpty else { return false }
      guard let value = Decimal(string: quantity), value > 0 else { return false }
      return true
    }
  }

  private var familyIcon: String {
    switch product.family {
    case .mass: "scalemass"
    case .volume: "drop"
    case .count: "number"
    }
  }

  private func loadInitial() async {
    if productPackageSize != nil {
      entryMode = .package
    }
    if unitCode.isEmpty {
      unitCode = product.preferredUnit
    }
    async let locationReq = appState.api.locations()
    async let vesselReq = appState.api.storageVessels()
    if let loaded = try? await (locationReq, vesselReq) {
      locations = loaded.0.sorted { $0.sortOrder < $1.sortOrder }
      storageVessels = loaded.1.sorted { $0.sortOrder < $1.sortOrder }
      if selectedLocationID == nil {
        selectedLocationID = locations.first?.id
      }
    }
  }

  private func submit() async {
    guard let locationID = selectedLocationID else { return }
    isSubmitting = true
    errorMessage = nil
    do {
      let requests = createStockRequests(locationID: locationID)
      guard !requests.isEmpty else {
        isSubmitting = false
        return
      }
      var created: StockBatch?
      for request in requests {
        let batch = try await appState.api.createStock(request)
        if created == nil { created = batch }
      }
      await appState.refreshRemindersAfterInventoryMutation()
      if let created {
        onAdded?(created)
      }
      dismiss()
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
    isSubmitting = false
  }

  private func createStockRequests(locationID: String) -> [CreateStockRequest] {
    guard let stockAmount = stockQuantityAndUnit() else { return [] }
    let request = CreateStockRequest(
      expiresOn: hasExpiry ? StockBatch.yyyymmdd.string(from: expiry) : nil,
      locationId: locationID,
      note: note.trimmingCharacters(in: .whitespaces).isEmpty ? nil : note,
      openedOn: hasOpened ? StockBatch.yyyymmdd.string(from: opened) : nil,
      producedOn: hasProduced ? StockBatch.yyyymmdd.string(from: produced) : nil,
      productId: product.id,
      quantity: stockAmount.quantity,
      quantityIncludesStorageVessel: grossVesselWeightEligible && quantityIncludesStorageVessel
        ? true : nil,
      storageVesselId: selectedStorageVesselID,
      unit: stockAmount.unit,
    )
    guard entryMode == .package, let count = wholePackageCount() else {
      return [request]
    }
    return Array(repeating: request, count: count)
  }

  private func stockQuantityAndUnit() -> (quantity: String, unit: String)? {
    if entryMode == .package {
      guard
        let packageSize = productPackageSize,
        wholePackageCount() != nil
      else { return nil }
      return (packageSize.quantity, packageSize.unit)
    }
    guard !unitCode.isEmpty else { return nil }
    return (quantity, unitCode)
  }

  private func wholePackageCount() -> Int? {
    guard let value = Decimal(string: packageCount), value > 0 else { return nil }
    var copy = value
    var rounded = Decimal()
    NSDecimalRound(&rounded, &copy, 0, .plain)
    guard rounded == value, rounded <= Decimal(Int.max) else { return nil }
    let count = NSDecimalNumber(decimal: rounded).intValue
    return count > 0 ? count : nil
  }
}
