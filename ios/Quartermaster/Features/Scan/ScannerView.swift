import SwiftUI
import VisionKit

enum ScannerRecognitionMode {
  case barcode
  case expiryText

  var recognizedDataTypes: Set<DataScannerViewController.RecognizedDataType> {
    switch self {
    case .barcode: [.barcode()]
    case .expiryText: [.text()]
    }
  }

  var recognizesMultipleItems: Bool {
    switch self {
    case .barcode: false
    case .expiryText: true
    }
  }
}

/// Thin UIKit bridge around `DataScannerViewController`. The parent view owns
/// flow state and debouncing; this just forwards recognised payloads.
struct ScannerView: UIViewControllerRepresentable {
  var mode: ScannerRecognitionMode = .barcode
  var isScanning: Bool = true
  var onBarcode: (String) -> Void = { _ in }
  var onText: (String) -> Void = { _ in }

  func makeUIViewController(context: Context) -> DataScannerViewController {
    let controller = DataScannerViewController(
      recognizedDataTypes: mode.recognizedDataTypes,
      qualityLevel: .balanced,
      recognizesMultipleItems: mode.recognizesMultipleItems,
      isHighFrameRateTrackingEnabled: false,
      isPinchToZoomEnabled: true,
      isGuidanceEnabled: true,
      isHighlightingEnabled: true,
    )
    controller.delegate = context.coordinator
    return controller
  }

  func updateUIViewController(_ uiViewController: DataScannerViewController, context: Context) {
    context.coordinator.onBarcode = onBarcode
    context.coordinator.onText = onText
    if isScanning {
      try? uiViewController.startScanning()
    } else {
      uiViewController.stopScanning()
    }
  }

  static func dismantleUIViewController(
    _ uiViewController: DataScannerViewController,
    coordinator: Coordinator
  ) {
    uiViewController.stopScanning()
  }

  func makeCoordinator() -> Coordinator {
    Coordinator(onBarcode: onBarcode, onText: onText)
  }

  final class Coordinator: NSObject, DataScannerViewControllerDelegate {
    var onBarcode: (String) -> Void
    var onText: (String) -> Void

    init(onBarcode: @escaping (String) -> Void, onText: @escaping (String) -> Void) {
      self.onBarcode = onBarcode
      self.onText = onText
    }

    func dataScanner(
      _ dataScanner: DataScannerViewController,
      didAdd addedItems: [RecognizedItem],
      allItems: [RecognizedItem],
    ) {
      for item in addedItems {
        if case .barcode(let code) = item, let value = code.payloadStringValue {
          onBarcode(value)
        }
      }
      forwardText(from: allItems)
    }

    func dataScanner(
      _ dataScanner: DataScannerViewController,
      didUpdate updatedItems: [RecognizedItem],
      allItems: [RecognizedItem],
    ) {
      forwardText(from: allItems)
    }

    private func forwardText(from items: [RecognizedItem]) {
      let transcript =
        items
        .compactMap { item in
          if case .text(let text) = item {
            return text.transcript
          }
          return nil
        }
        .joined(separator: "\n")
      if !transcript.isEmpty {
        onText(transcript)
      }
    }
  }
}

struct ScanScreen: View {
  @Environment(AppState.self) private var appState

  private enum QuantityEntryMode: String, CaseIterable, Identifiable {
    case package
    case exact

    var id: String { rawValue }
  }

  private struct ScannedStockDraft: Equatable {
    var product: Product
    var entryMode: QuantityEntryMode
    var packageCount: String
    var quantity: String
    var unitCode: String
    var locationID: String?
    var expiryCandidate: ExpiryDateCandidate?
  }

  @State private var lookupTask: Task<Void, Never>?
  @State private var lastHandled: String = ""
  @State private var sheet: Route?
  @State private var scannerSessionID = UUID()
  @State private var scannerMode: ScannerRecognitionMode = .barcode
  @State private var draft: ScannedStockDraft?
  @State private var expiryCandidates: [ExpiryDateCandidate] = []
  @State private var scannedExpiryText = ""
  @State private var locations: [Location] = []
  @State private var errorMessage: String?
  @State private var noticeMessage: String?
  @State private var suppressNextSheetDismissReset = false
  @State private var isLooking = false
  @State private var isSubmitting = false
  @State private var isSwitchingHousehold = false

  enum Route: Identifiable, Hashable {
    case addStock(Product)
    case manualCreate(String)

    var id: String {
      switch self {
      case .addStock(let p): "addStock-\(p.id)"
      case .manualCreate(let code): "manual-\(code)"
      }
    }
  }

  var body: some View {
    ZStack {
      if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
        ScannerView(
          mode: scannerMode,
          isScanning: shouldScan,
          onBarcode: handleBarcode,
          onText: handleExpiryText
        )
        .id(scannerSessionID)
        .ignoresSafeArea(edges: [.bottom, .horizontal])
        if isLooking {
          ProgressView("Looking up…")
            .padding()
            .background(.thinMaterial, in: Capsule())
        }
        overlayPanel
      } else {
        ContentUnavailableView {
          Label("Camera scanning unavailable", systemImage: "camera.slash")
        } description: {
          Text(
            "Barcode scanning requires a physical device with a camera. In the simulator, tap + on the Inventory tab to add stock by product search."
          )
        }
        .padding()
      }
    }
    .navigationTitle("Scan")
    .toolbar {
      ToolbarItem(placement: .topBarLeading) {
        if let me = appState.me {
          HouseholdSwitcherMenu(
            me: me,
            isSwitching: isSwitchingHousehold,
            onSwitch: switchHousehold
          )
        }
      }
    }
    .alert(
      "Couldn't look up barcode",
      isPresented: Binding(
        get: { errorMessage != nil },
        set: { if !$0 { errorMessage = nil } }
      )
    ) {
      Button("OK", role: .cancel) {}
    } message: {
      Text(errorMessage ?? "")
    }
    .sheet(item: $sheet, onDismiss: handleSheetDismiss) { route in
      switch route {
      case .addStock(let product):
        AddStockView(product: product) { _ in
          lastHandled = ""
          resetScanner()
        }
      case .manualCreate(let barcode):
        ManualProductForm(prefillBarcode: barcode) { created in
          suppressNextSheetDismissReset = true
          sheet = nil
          Task {
            await ensureLocationsLoaded()
            beginDraft(for: created)
          }
        }
      }
    }
  }

  private var shouldScan: Bool {
    guard sheet == nil, !isLooking, !isSubmitting else { return false }
    return draft == nil || scannerMode == .expiryText
  }

  @ViewBuilder
  private var overlayPanel: some View {
    VStack {
      Spacer()
      if scannerMode == .expiryText {
        expiryScanPanel
      } else if draft != nil {
        draftPanel
      } else if let noticeMessage {
        Text(noticeMessage)
          .font(.subheadline.weight(.medium))
          .padding(.horizontal, 14)
          .padding(.vertical, 10)
          .background(.regularMaterial, in: Capsule())
          .padding(.bottom, 20)
      }
    }
    .frame(maxWidth: .infinity)
  }

  private var draftPanel: some View {
    VStack(alignment: .leading, spacing: 14) {
      if let draft {
        productSummary(draft.product)
        quantityControls(for: draft)
        locationPicker
        expirySummary
        actionButtons
      }
    }
    .padding()
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.regularMaterial)
  }

  private var expiryScanPanel: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Scan expiry date")
        .font(.headline)
      if expiryCandidates.isEmpty {
        Label("Point the camera at the printed expiry date.", systemImage: "calendar.badge.clock")
          .font(.subheadline)
          .foregroundStyle(.secondary)
      } else {
        ForEach(expiryCandidates.prefix(3)) { candidate in
          Button {
            draft?.expiryCandidate = candidate
            showDraftReview()
          } label: {
            HStack(spacing: 12) {
              VStack(alignment: .leading, spacing: 2) {
                Text(StockBatch.yyyymmdd.string(from: candidate.date))
                  .font(.body.weight(.semibold))
                Text("Read from \(candidate.sourceText)")
                  .font(.caption)
                  .foregroundStyle(.secondary)
                if candidate.precision == .month {
                  Text("Using the last day of the printed month")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
              }
              Spacer()
              Image(systemName: "checkmark.circle")
                .foregroundStyle(QuartermasterBrand.green600)
            }
          }
          .buttonStyle(.plain)
          .padding(.vertical, 4)
        }
      }
      HStack {
        Button("Back") { showDraftReview() }
        Spacer()
        Button("Skip expiry") {
          draft?.expiryCandidate = nil
          showDraftReview()
        }
        .fontWeight(.semibold)
      }
    }
    .padding()
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.regularMaterial)
  }

  private func productSummary(_ product: Product) -> some View {
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
        Text(product.displayTitle)
          .font(.headline)
        HStack(spacing: 4) {
          Image(systemName: familyIcon(for: product))
          Text(product.family.displayName)
          if let barcode = product.barcode {
            Text("·")
            Text(barcode).monospaced()
          }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
      }
      Spacer()
    }
  }

  @ViewBuilder
  private func quantityControls(for draft: ScannedStockDraft) -> some View {
    VStack(alignment: .leading, spacing: 8) {
      if productPackageSize(for: draft.product) != nil {
        Picker(
          "Entry",
          selection: Binding(
            get: { self.draft?.entryMode ?? .exact },
            set: { self.draft?.entryMode = $0 }
          )
        ) {
          Text("Packages").tag(QuantityEntryMode.package)
          Text("Exact amount").tag(QuantityEntryMode.exact)
        }
        .pickerStyle(.segmented)
      }
      if draft.entryMode == .package,
        let packageSize = productPackageSize(for: draft.product)
      {
        DecimalField(
          title: "Packages",
          text: Binding(
            get: { self.draft?.packageCount ?? "" },
            set: { self.draft?.packageCount = $0 }
          )
        )
        .textFieldStyle(.roundedBorder)
        Text("Each package adds \(packageSize.quantity) \(packageSize.unit).")
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        HStack {
          DecimalField(
            title: "Amount",
            text: Binding(
              get: { self.draft?.quantity ?? "" },
              set: { self.draft?.quantity = $0 }
            )
          )
          .textFieldStyle(.roundedBorder)
          Picker(
            "Unit",
            selection: Binding(
              get: { self.draft?.unitCode ?? draft.product.preferredUnit },
              set: { self.draft?.unitCode = $0 }
            )
          ) {
            ForEach(appState.unitsFor(family: draft.product.family), id: \.code) { unit in
              Text(unit.code).tag(unit.code)
            }
          }
          .labelsHidden()
        }
      }
    }
  }

  private var locationPicker: some View {
    Picker(
      "Location",
      selection: Binding(
        get: { draft?.locationID },
        set: { draft?.locationID = $0 }
      )
    ) {
      ForEach(locations) { loc in
        Text(loc.name).tag(Optional(loc.id))
      }
    }
  }

  private var expirySummary: some View {
    HStack {
      VStack(alignment: .leading, spacing: 2) {
        Text("Expiry")
          .font(.subheadline.weight(.semibold))
        if let candidate = draft?.expiryCandidate {
          Text(StockBatch.yyyymmdd.string(from: candidate.date))
            .font(.subheadline)
            .foregroundStyle(.secondary)
        } else {
          Text("Not set")
            .font(.subheadline)
            .foregroundStyle(.secondary)
        }
      }
      Spacer()
      Button {
        showExpiryScan()
      } label: {
        Label(draft?.expiryCandidate == nil ? "Scan" : "Rescan", systemImage: "text.viewfinder")
      }
      .buttonStyle(.bordered)
    }
  }

  private var actionButtons: some View {
    HStack {
      Button("Cancel", role: .cancel) { resetScanner() }
      Button("Edit details") {
        if let product = draft?.product {
          sheet = .addStock(product)
        }
      }
      Spacer()
      Button {
        Task { await submitDraft() }
      } label: {
        if isSubmitting {
          ProgressView()
        } else {
          Text("Add stock")
            .fontWeight(.semibold)
        }
      }
      .buttonStyle(.borderedProminent)
      .disabled(!canSubmitDraft || isSubmitting)
    }
  }

  private var canSubmitDraft: Bool {
    guard let draft, draft.locationID != nil else { return false }
    if draft.entryMode == .package {
      guard productPackageSize(for: draft.product) != nil else { return false }
      return wholePackageCount(draft.packageCount) != nil
    }
    guard !draft.quantity.isEmpty, !draft.unitCode.isEmpty else { return false }
    guard let value = Decimal(string: draft.quantity), value > 0 else { return false }
    return true
  }

  private func handleBarcode(_ code: String) {
    if scannerMode != .barcode || draft != nil || sheet != nil || code == lastHandled || isLooking {
      return
    }
    lastHandled = code
    lookupTask?.cancel()
    lookupTask = Task {
      try? await Task.sleep(for: .milliseconds(200))
      if Task.isCancelled { return }
      isLooking = true
      defer { isLooking = false }
      do {
        let response = try await appState.api.lookupBarcode(code)
        await ensureLocationsLoaded()
        beginDraft(for: response.product)
      } catch let err as APIError {
        if case .server(let status, _) = err, status == 404 {
          sheet = .manualCreate(code)
        } else {
          errorMessage = err.userFacingMessage
          lastHandled = ""
        }
      } catch {
        errorMessage = error.localizedDescription
        lastHandled = ""
      }
    }
  }

  private func handleExpiryText(_ text: String) {
    guard scannerMode == .expiryText, text != scannedExpiryText else { return }
    scannedExpiryText = text
    expiryCandidates = ExpiryDateParser.candidates(in: text)
  }

  private func beginDraft(for product: Product) {
    let packageSize = productPackageSize(for: product)
    draft = ScannedStockDraft(
      product: product,
      entryMode: packageSize == nil ? .exact : .package,
      packageCount: packageSize == nil ? "" : "1",
      quantity: packageSize == nil ? "1" : "",
      unitCode: product.preferredUnit,
      locationID: locations.first?.id,
      expiryCandidate: nil,
    )
    showDraftReview()
  }

  private func showExpiryScan() {
    scannedExpiryText = ""
    expiryCandidates = []
    scannerMode = .expiryText
    scannerSessionID = UUID()
  }

  private func showDraftReview() {
    scannerMode = .barcode
    scannerSessionID = UUID()
  }

  private func submitDraft() async {
    guard let draft else { return }
    isSubmitting = true
    errorMessage = nil
    do {
      let requests = createStockRequests(from: draft)
      guard !requests.isEmpty else {
        isSubmitting = false
        return
      }
      for request in requests {
        _ = try await appState.api.createStock(request)
      }
      await appState.refreshRemindersAfterInventoryMutation()
      noticeMessage = "Added \(draft.product.name)"
      resetScanner(keepingNotice: true)
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
    isSubmitting = false
  }

  private func createStockRequests(from draft: ScannedStockDraft) -> [CreateStockRequest] {
    guard let locationID = draft.locationID, let stockAmount = stockQuantityAndUnit(from: draft)
    else { return [] }
    let request = CreateStockRequest(
      expiresOn: draft.expiryCandidate.map { StockBatch.yyyymmdd.string(from: $0.date) },
      locationId: locationID,
      note: nil,
      openedOn: nil,
      producedOn: nil,
      productId: draft.product.id,
      quantity: stockAmount.quantity,
      unit: stockAmount.unit,
    )
    guard draft.entryMode == .package, let count = wholePackageCount(draft.packageCount) else {
      return [request]
    }
    return Array(repeating: request, count: count)
  }

  private func stockQuantityAndUnit(from draft: ScannedStockDraft) -> (
    quantity: String, unit: String
  )? {
    if draft.entryMode == .package {
      guard
        let packageSize = productPackageSize(for: draft.product),
        wholePackageCount(draft.packageCount) != nil
      else { return nil }
      return (packageSize.quantity, packageSize.unit)
    }
    guard !draft.unitCode.isEmpty else { return nil }
    return (draft.quantity, draft.unitCode)
  }

  private func productPackageSize(for product: Product) -> (quantity: String, unit: String)? {
    guard
      let quantity = product.packageQuantity,
      let unit = product.packageUnit,
      Decimal(string: quantity).map({ $0 > 0 }) == true
    else { return nil }
    return (quantity, unit)
  }

  private func wholePackageCount(_ text: String) -> Int? {
    guard let value = Decimal(string: text), value > 0 else { return nil }
    var copy = value
    var rounded = Decimal()
    NSDecimalRound(&rounded, &copy, 0, .plain)
    guard rounded == value, rounded <= Decimal(Int.max) else { return nil }
    let count = NSDecimalNumber(decimal: rounded).intValue
    return count > 0 ? count : nil
  }

  private func ensureLocationsLoaded() async {
    if !locations.isEmpty { return }
    if let loaded = try? await appState.api.locations() {
      locations = loaded.sorted { $0.sortOrder < $1.sortOrder }
    }
  }

  private func handleSheetDismiss() {
    if suppressNextSheetDismissReset {
      suppressNextSheetDismissReset = false
      return
    }
    resetScanner()
  }

  private func familyIcon(for product: Product) -> String {
    switch product.family {
    case .mass: "scalemass"
    case .volume: "drop"
    case .count: "number"
    }
  }

  private func resetScanner() {
    resetScanner(keepingNotice: false)
  }

  private func resetScanner(keepingNotice: Bool) {
    lookupTask?.cancel()
    lookupTask = nil
    isLooking = false
    isSubmitting = false
    lastHandled = ""
    scannerMode = .barcode
    draft = nil
    expiryCandidates = []
    scannedExpiryText = ""
    if !keepingNotice {
      noticeMessage = nil
    }
    scannerSessionID = UUID()
  }

  private func switchHousehold(to householdID: String) {
    guard !isSwitchingHousehold else { return }
    isSwitchingHousehold = true
    Task {
      defer { isSwitchingHousehold = false }
      do {
        _ = try await appState.switchHousehold(to: householdID)
        sheet = nil
        resetScanner()
      } catch {
        errorMessage = (error as? APIError)?.userFacingMessage ?? error.localizedDescription
      }
    }
  }
}
