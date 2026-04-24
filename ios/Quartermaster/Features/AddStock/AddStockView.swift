import SwiftUI

struct AddStockView: View {
  @Environment(AppState.self) private var appState
  @Environment(\.dismiss) private var dismiss

  let product: Product
  var onAdded: ((StockBatch) -> Void)?

  @State private var quantity: String = ""
  @State private var unitCode: String = ""
  @State private var selectedLocationID: String?
  @State private var hasExpiry: Bool = false
  @State private var expiry: Date =
    Calendar.current.date(byAdding: .day, value: 30, to: .now) ?? .now
  @State private var hasOpened: Bool = false
  @State private var opened: Date = .now
  @State private var note: String = ""

  @State private var locations: [Location] = []
  @State private var isSubmitting = false
  @State private var errorMessage: String?

  var body: some View {
    NavigationStack {
      Form {
        Section("Product") {
          productHeader
        }
        Section("Quantity") {
          DecimalField(title: "Amount", text: $quantity)
          Picker("Unit", selection: $unitCode) {
            ForEach(unitOptions, id: \.code) { u in
              Text(u.code).tag(u.code)
            }
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
          Toggle("Set expiry date", isOn: $hasExpiry.animation())
          if hasExpiry {
            DatePicker("Expires", selection: $expiry, displayedComponents: .date)
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
            Text(msg).foregroundStyle(.red)
          }
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

  private var canSubmit: Bool {
    guard !quantity.isEmpty, selectedLocationID != nil, !unitCode.isEmpty else { return false }
    guard let value = Decimal(string: quantity), value > 0 else { return false }
    return true
  }

  private var familyIcon: String {
    switch product.family {
    case .mass: "scalemass"
    case .volume: "drop"
    case .count: "number"
    }
  }

  private func loadInitial() async {
    if unitCode.isEmpty {
      unitCode = product.preferredUnit
    }
    if locations.isEmpty {
      if let l = try? await appState.api.locations() {
        locations = l.sorted { $0.sortOrder < $1.sortOrder }
        if selectedLocationID == nil {
          selectedLocationID = locations.first?.id
        }
      }
    }
  }

  private func submit() async {
    guard let locationID = selectedLocationID else { return }
    isSubmitting = true
    errorMessage = nil
    let req = CreateStockRequest(
      expiresOn: hasExpiry ? StockBatch.yyyymmdd.string(from: expiry) : nil,
      locationId: locationID,
      note: note.trimmingCharacters(in: .whitespaces).isEmpty ? nil : note,
      openedOn: hasOpened ? StockBatch.yyyymmdd.string(from: opened) : nil,
      productId: product.id,
      quantity: quantity,
      unit: unitCode,
    )
    do {
      let created = try await appState.api.createStock(req)
      await appState.refreshRemindersAfterInventoryMutation()
      onAdded?(created)
      dismiss()
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
    isSubmitting = false
  }
}
