import SwiftUI
import VisionKit

struct ExpiryDateScannerView: View {
  @Environment(\.dismiss) private var dismiss

  var onSelect: (ExpiryDateCandidate) -> Void

  @State private var candidates: [ExpiryDateCandidate] = []
  @State private var scannedText = ""

  var body: some View {
    NavigationStack {
      ZStack(alignment: .bottom) {
        if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
          ExpiryTextScannerView(onText: updateCandidates)
            .ignoresSafeArea(edges: [.bottom, .horizontal])
        } else {
          ContentUnavailableView {
            Label("Text scanning unavailable", systemImage: "camera.slash")
          } description: {
            Text("Expiry date scanning requires a physical device with a camera.")
          }
          .padding()
        }

        candidatePanel
      }
      .navigationTitle("Scan expiry date")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Cancel") { dismiss() }
        }
      }
    }
  }

  private var candidatePanel: some View {
    VStack(alignment: .leading, spacing: 12) {
      if candidates.isEmpty {
        Label("Point the camera at the printed expiry date.", systemImage: "calendar.badge.clock")
          .font(.subheadline)
          .foregroundStyle(.secondary)
      } else {
        Text("Detected dates")
          .font(.headline)
        ForEach(candidates.prefix(3)) { candidate in
          Button {
            onSelect(candidate)
            dismiss()
          } label: {
            HStack {
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
          .padding(.vertical, 6)
        }
      }
    }
    .padding()
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.regularMaterial)
  }

  private func updateCandidates(_ text: String) {
    guard text != scannedText else { return }
    scannedText = text
    candidates = ExpiryDateParser.candidates(in: text)
  }
}

private struct ExpiryTextScannerView: UIViewControllerRepresentable {
  var onText: (String) -> Void

  func makeUIViewController(context: Context) -> DataScannerViewController {
    let controller = DataScannerViewController(
      recognizedDataTypes: [.text()],
      qualityLevel: .balanced,
      recognizesMultipleItems: true,
      isHighFrameRateTrackingEnabled: false,
      isPinchToZoomEnabled: true,
      isGuidanceEnabled: true,
      isHighlightingEnabled: true,
    )
    controller.delegate = context.coordinator
    return controller
  }

  func updateUIViewController(_ uiViewController: DataScannerViewController, context: Context) {
    context.coordinator.onText = onText
    try? uiViewController.startScanning()
  }

  static func dismantleUIViewController(
    _ uiViewController: DataScannerViewController,
    coordinator: Coordinator
  ) {
    uiViewController.stopScanning()
  }

  func makeCoordinator() -> Coordinator {
    Coordinator(onText: onText)
  }

  final class Coordinator: NSObject, DataScannerViewControllerDelegate {
    var onText: (String) -> Void

    init(onText: @escaping (String) -> Void) {
      self.onText = onText
    }

    func dataScanner(
      _ dataScanner: DataScannerViewController,
      didAdd addedItems: [RecognizedItem],
      allItems: [RecognizedItem],
    ) {
      onText(Self.transcript(from: allItems))
    }

    func dataScanner(
      _ dataScanner: DataScannerViewController,
      didUpdate updatedItems: [RecognizedItem],
      allItems: [RecognizedItem],
    ) {
      onText(Self.transcript(from: allItems))
    }

    private static func transcript(from items: [RecognizedItem]) -> String {
      items.compactMap { item in
        if case .text(let text) = item {
          return text.transcript
        }
        return nil
      }
      .joined(separator: "\n")
    }
  }
}
