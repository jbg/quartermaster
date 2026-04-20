import SwiftUI
import VisionKit

/// Thin UIKit bridge around `DataScannerViewController`. The parent view is
/// responsible for debouncing + lookup; this just forwards every recognised
/// barcode payload.
struct ScannerView: UIViewControllerRepresentable {
    var onBarcode: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let controller = DataScannerViewController(
            recognizedDataTypes: [.barcode()],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true,
            isGuidanceEnabled: true,
            isHighlightingEnabled: true,
        )
        controller.delegate = context.coordinator
        return controller
    }

    func updateUIViewController(_ uiViewController: DataScannerViewController, context: Context) {
        try? uiViewController.startScanning()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onBarcode: onBarcode)
    }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onBarcode: (String) -> Void

        init(onBarcode: @escaping (String) -> Void) {
            self.onBarcode = onBarcode
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
        }
    }
}

struct ScanScreen: View {
    @Environment(AppState.self) private var appState

    @State private var lookupTask: Task<Void, Never>?
    @State private var lastHandled: String = ""
    @State private var sheet: Route?
    @State private var errorMessage: String?
    @State private var isLooking = false

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
                ScannerView(onBarcode: handleBarcode)
                    .ignoresSafeArea(edges: [.bottom, .horizontal])
                if isLooking {
                    ProgressView("Looking up…")
                        .padding()
                        .background(.thinMaterial, in: Capsule())
                }
            } else {
                ContentUnavailableView {
                    Label("Camera scanning unavailable", systemImage: "camera.slash")
                } description: {
                    Text("Barcode scanning requires a physical device with a camera. In the simulator, tap + on the Inventory tab to add stock by product search.")
                }
                .padding()
            }
        }
        .navigationTitle("Scan")
        .alert("Couldn't look up barcode", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(errorMessage ?? "")
        }
        .sheet(item: $sheet) { route in
            switch route {
            case .addStock(let product):
                AddStockView(product: product) { _ in
                    lastHandled = ""
                }
            case .manualCreate(let barcode):
                ManualProductForm(prefillBarcode: barcode) { created in
                    sheet = .addStock(created)
                }
            }
        }
    }

    private func handleBarcode(_ code: String) {
        if code == lastHandled || isLooking { return }
        lastHandled = code
        lookupTask?.cancel()
        lookupTask = Task {
            try? await Task.sleep(for: .milliseconds(200))
            if Task.isCancelled { return }
            isLooking = true
            defer { isLooking = false }
            do {
                let response = try await appState.api.lookupBarcode(code)
                sheet = .addStock(response.product)
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
}
