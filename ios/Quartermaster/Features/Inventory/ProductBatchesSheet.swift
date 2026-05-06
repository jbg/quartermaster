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
  @State private var labelPrintingBatchID: String?
  @State private var labelPrintNotice: LabelPrintNotice?
  /// Batch currently showing the "you came from here" flash. Cleared by a
  /// background task after ~1.5 s so the pulse feels transient.
  @State private var flashingBatchID: String?

  var body: some View {
    NavigationStack {
      ScrollViewReader { proxy in
        List {
          ForEach(batches) { batch in
            batchListRow(batch)
          }
          Section {
            Button {
              consumeTarget = batches.first(where: { !isDepleted($0) })
            } label: {
              Label("Use stock", systemImage: "minus.circle")
            }
            .accessibilityIdentifier("batch.consume")
            .disabled(!batches.contains(where: { !isDepleted($0) }))
          }
        }
        .task(id: highlightBatchID) { await flashHighlight(proxy: proxy) }
      }
      .navigationTitle(product.displayTitle)
      .accessibilityIdentifier("batch.sheet")
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
      .sheet(item: $consumeTarget) { batch in
        ConsumeForm(product: product, location: location, batch: batch) {
          await onMutated()
          if let refreshed = try? await appState.api.listStock(
            locationID: location.id, productID: product.id, includeDepleted: true)
          {
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
              includeDepleted: true,
            ) {
              batches = refreshed
            }
          }
        }
      }
      .alert(
        "Couldn't update stock",
        isPresented: Binding(
          get: { errorMessage != nil },
          set: { if !$0 { errorMessage = nil } }
        )
      ) {
        Button("OK", role: .cancel) {}
      } message: {
        Text(errorMessage ?? "")
      }
      .alert(item: $labelPrintNotice) { notice in
        Alert(
          title: Text(notice.title),
          message: Text(notice.message),
          dismissButton: .default(Text("OK")),
        )
      }
    }
  }

  private func batchListRow(_ batch: StockBatch) -> some View {
    BatchRow(
      batch: batch,
      isPrintingLabel: labelPrintingBatchID == batch.id
    ) { includeQuantity in
      Task { await printLabel(for: batch, includeQuantity: includeQuantity) }
    }
    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
      if !isDepleted(batch) {
        Button(role: .destructive) {
          Task { await delete(batch) }
        } label: {
          Label("Delete", systemImage: "trash")
        }
      }
    }
    .swipeActions(edge: .leading) {
      if !isDepleted(batch) {
        Button {
          editing = batch
        } label: {
          Label("Edit", systemImage: "pencil")
        }
        .tint(QuartermasterBrand.blueprint)
      }
    }
    .contentShape(Rectangle())
    .onTapGesture {
      if isDepleted(batch) {
        showBatchHistory = batch
      } else {
        editing = batch
      }
    }
    .listRowBackground(
      flashingBatchID == batch.id
        ? QuartermasterBrand.sage100
        : Color.clear,
    )
    .animation(.easeOut(duration: 0.4), value: flashingBatchID)
    .id(batch.id)
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
      await appState.refreshRemindersAfterInventoryMutation()
      await onMutated()
      if batches.isEmpty { dismiss() }
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func printLabel(for batch: StockBatch, includeQuantity: Bool) async {
    guard labelPrintingBatchID == nil else { return }
    labelPrintingBatchID = batch.id
    defer { labelPrintingBatchID = nil }
    do {
      let response = try await appState.api.printStockLabel(
        id: batch.id,
        copies: 1,
        includeQuantity: includeQuantity,
      )
      let action = response.status == .sent ? "sent" : "rendered"
      labelPrintNotice = LabelPrintNotice(
        title: "Label \(action)",
        message: response.batchUrl,
      )
    } catch let err as APIError {
      labelPrintNotice = LabelPrintNotice(
        title: "Couldn't print label",
        message: labelPrintErrorMessage(err),
      )
    } catch {
      labelPrintNotice = LabelPrintNotice(
        title: "Couldn't print label",
        message: error.localizedDescription,
      )
    }
  }

  private func labelPrintErrorMessage(_ error: APIError) -> String {
    if case .server(_, let body?) = error {
      if body.code == "bad_request" {
        if body.message.contains("QM_PUBLIC_BASE_URL") {
          return "Set QM_PUBLIC_BASE_URL before printing QR labels."
        }
        if body.message.contains("no enabled label printer") {
          return "Add an enabled label printer from the web Settings screen first."
        }
      }
    }
    return error.userFacingMessage
  }
}

private struct LabelPrintNotice: Identifiable {
  let id = UUID()
  let title: String
  let message: String
}

struct BatchRow: View {
  let batch: StockBatch
  var isPrintingLabel = false
  var onPrintLabel: ((Bool) -> Void)?

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
        if let produced = batch.producedOnDate {
          Label("Prepared \(Self.relativeDate(produced))", systemImage: "fork.knife")
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
        if isDepleted(batch) {
          Label("Depleted", systemImage: "archivebox")
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.secondary)
        }
      }
      Spacer()
      ExpiryBadge(expiresOn: batch.expiresOn)
      if let onPrintLabel {
        if isPrintingLabel {
          ProgressView()
        } else {
          Menu {
            Button {
              onPrintLabel(false)
            } label: {
              Label("Print label", systemImage: "qrcode")
            }
            Button {
              onPrintLabel(true)
            } label: {
              Label("Print with quantity", systemImage: "number")
            }
          } label: {
            Image(systemName: "qrcode")
          }
          .buttonStyle(.borderless)
          .foregroundStyle(QuartermasterBrand.green600)
          .accessibilityLabel("Print label for this batch")
          .accessibilityIdentifier("batch.print-label.\(batch.id)")
        }
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityIdentifier(
      isDepleted(batch) ? "batch.row.depleted.\(batch.id)" : "batch.row.active.\(batch.id)"
    )
    .padding(.vertical, 2)
  }

  private static func relativeDate(_ d: Date) -> String {
    let formatter = DateFormatter()
    formatter.dateStyle = .medium
    formatter.timeStyle = .none
    return formatter.string(from: d)
  }
}

private func isDepleted(_ batch: StockBatch) -> Bool {
  batch.depletedAt != nil
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
  @State private var showExpiryScanner = false
  @State private var hasProduced: Bool
  @State private var produced: Date
  @State private var hadProducedOriginally: Bool
  @State private var hasOpened: Bool
  @State private var opened: Date
  @State private var hadOpenedOriginally: Bool
  @State private var note: String
  @State private var isSubmitting = false
  @State private var errorMessage: String?

  init(
    batch: StockBatch, product: Product, locations: [Location],
    onUpdated: @escaping (StockBatch) -> Void
  ) {
    self.batch = batch
    self.product = product
    self.locations = locations
    self.onUpdated = onUpdated
    _quantity = State(initialValue: batch.quantity)
    _locationID = State(initialValue: batch.locationID)
    let originalExpiry = batch.expiresOn != nil
    _hasExpiry = State(initialValue: originalExpiry)
    _hadExpiryOriginally = State(initialValue: originalExpiry)
    _expiry = State(
      initialValue: batch.expiresOnDate ?? Calendar.current.date(
        byAdding: .day, value: 30, to: .now) ?? .now)
    let originalProduced = batch.producedOn != nil
    _hasProduced = State(initialValue: originalProduced)
    _hadProducedOriginally = State(initialValue: originalProduced)
    _produced = State(initialValue: batch.producedOnDate ?? .now)
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
          Text(
            "The unit is fixed when the batch is added. Change it by deleting the batch and re-adding."
          )
        }
        Section("Location") {
          Picker("Location", selection: $locationID) {
            ForEach(locations) { loc in
              Text(loc.name).tag(loc.id)
            }
          }
        }
        Section {
          Toggle("Set prepared date", isOn: $hasProduced.animation())
          if hasProduced {
            DatePicker("Prepared on", selection: $produced, displayedComponents: .date)
          }
          if hadProducedOriginally && !hasProduced {
            Text("Saving will remove the prepared date from this batch.")
              .font(.footnote)
              .foregroundStyle(.secondary)
          }
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
          Section { Text(msg).foregroundStyle(Color.quartermasterError) }
        }
      }
      .navigationTitle("Edit batch")
      .navigationBarTitleDisplayMode(.inline)
      .sheet(isPresented: $showExpiryScanner) {
        ExpiryDateScannerView { candidate in
          expiry = candidate.date
          hasExpiry = true
        }
      }
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Cancel") { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
          Button {
            Task { await submit() }
          } label: {
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
      request.append(jsonPatchReplace("/quantity", quantity))
    }
    if locationID != batch.locationID {
      request.append(jsonPatchReplace("/location_id", locationID))
    }
    if hasProduced {
      let s = StockBatch.yyyymmdd.string(from: produced)
      if s != batch.producedOn {
        request.append(jsonPatchReplace("/produced_on", s))
      }
    } else if hadProducedOriginally {
      request.append(jsonPatchRemove("/produced_on"))
    }
    if hasExpiry {
      let s = StockBatch.yyyymmdd.string(from: expiry)
      if s != batch.expiresOn {
        request.append(jsonPatchReplace("/expires_on", s))
      }
    } else if hadExpiryOriginally {
      request.append(jsonPatchRemove("/expires_on"))
    }
    if hasOpened {
      let s = StockBatch.yyyymmdd.string(from: opened)
      if s != batch.openedOn {
        request.append(jsonPatchReplace("/opened_on", s))
      }
    } else if hadOpenedOriginally {
      request.append(jsonPatchRemove("/opened_on"))
    }
    let trimmedNote = note.trimmingCharacters(in: .whitespaces)
    if trimmedNote.isEmpty {
      if batch.note != nil {
        request.append(jsonPatchRemove("/note"))
      }
    } else if trimmedNote != batch.note {
      request.append(jsonPatchReplace("/note", trimmedNote))
    }

    do {
      let updated = try await appState.api.updateStock(id: batch.id, request: request)
      await appState.refreshRemindersAfterInventoryMutation()
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

  private enum QuantityEntryMode: String, CaseIterable, Identifiable {
    case package
    case exact

    var id: String { rawValue }
  }

  let product: Product
  let location: Location
  let batch: StockBatch
  var onConsumed: () async -> Void

  @State private var entryMode: QuantityEntryMode = .exact
  @State private var packageCount: String = ""
  @State private var quantity: String = ""
  @State private var unitCode: String
  @State private var isSubmitting = false
  @State private var errorMessage: String?
  @State private var successMessage: String?

  init(
    product: Product,
    location: Location,
    batch: StockBatch,
    onConsumed: @escaping () async -> Void
  ) {
    self.product = product
    self.location = location
    self.batch = batch
    self.onConsumed = onConsumed
    _unitCode = State(initialValue: product.preferredUnit)
    if batch.packageQuantity != nil && batch.packageUnit != nil {
      _entryMode = State(initialValue: .package)
    }
  }

  var body: some View {
    NavigationStack {
      Form {
        Section {
          if packageSize != nil {
            Picker("Entry", selection: $entryMode) {
              Text("Packages").tag(QuantityEntryMode.package)
              Text("Exact amount").tag(QuantityEntryMode.exact)
            }
            .pickerStyle(.segmented)
          }
          if entryMode == .package, let packageSize {
            DecimalField(title: "Packages", text: $packageCount)
            LabeledContent("Each", value: "\(packageSize.quantity) \(packageSize.unit)")
          } else {
            DecimalField(title: "Amount", text: $quantity)
            Picker("Unit", selection: $unitCode) {
              ForEach(appState.unitsFor(family: product.family), id: \.code) { u in
                Text(u.code).tag(u.code)
              }
            }
          }
        } header: {
          Text("How much did you use?")
        } footer: {
          if entryMode == .package, packageSize != nil {
            Text("Quartermaster will use the saved package size for this batch.")
          }
        }
        Section {
          Text("We'll take from the batch that expires soonest first.")
            .font(.footnote)
            .foregroundStyle(.secondary)
        }
        if let msg = errorMessage {
          Section { Text(msg).foregroundStyle(Color.quartermasterError) }
        }
      }
      .navigationTitle("Use \(product.name)")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Cancel") { dismiss() }
        }
        ToolbarItem(placement: .confirmationAction) {
          Button {
            Task { await submit() }
          } label: {
            if isSubmitting { ProgressView() } else { Text("Use").fontWeight(.semibold) }
          }
          .disabled(!canSubmit || isSubmitting)
        }
      }
      .alert(
        "Stock used",
        isPresented: Binding(
          get: { successMessage != nil },
          set: { if !$0 { successMessage = nil } }
        )
      ) {
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
    if entryMode == .package {
      guard packageSize != nil else { return false }
      guard let value = Decimal(string: packageCount), value > 0 else { return false }
      return true
    } else {
      guard !quantity.isEmpty, let value = Decimal(string: quantity), value > 0 else {
        return false
      }
      return true
    }
  }

  private var packageSize: (quantity: String, unit: String)? {
    guard
      let quantity = batch.packageQuantity,
      let unit = batch.packageUnit,
      Decimal(string: quantity).map({ $0 > 0 }) == true
    else { return nil }
    return (quantity, unit)
  }

  private func submit() async {
    guard let stockAmount = stockQuantityAndUnit() else { return }
    isSubmitting = true
    errorMessage = nil
    let request = ConsumeRequest(
      locationId: location.id,
      productId: product.id,
      quantity: stockAmount.quantity,
      unit: stockAmount.unit,
    )
    do {
      let response = try await appState.api.consumeStock(request)
      await appState.refreshRemindersAfterInventoryMutation()
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
    let totalLabel: String
    if entryMode == .package, let packageSize, let count = Decimal(string: packageCount) {
      let packageLabel = "package\(count == 1 ? "" : "s")"
      totalLabel =
        "\(Self.format(count)) \(packageLabel) (\(Self.format(total)) \(packageSize.unit))"
    } else {
      totalLabel = "\(Self.format(total)) \(unitCode)"
    }
    let count = response.consumed.count
    if count <= 1 {
      return "Used \(totalLabel)."
    }
    return "Used \(totalLabel) across \(count) batches."
  }

  private func stockQuantityAndUnit() -> (quantity: String, unit: String)? {
    if entryMode == .package {
      guard
        let packageSize,
        let count = Decimal(string: packageCount),
        let quantityPerPackage = Decimal(string: packageSize.quantity)
      else { return nil }
      return (Self.format(count * quantityPerPackage), packageSize.unit)
    }
    guard !unitCode.isEmpty else { return nil }
    return (quantity, unitCode)
  }

  private static func format(_ d: Decimal) -> String {
    var copy = d
    var rounded = Decimal()
    NSDecimalRound(&rounded, &copy, 3, .plain)
    return NSDecimalNumber(decimal: rounded).stringValue
  }
}
