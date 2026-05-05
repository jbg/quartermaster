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
          ScannerView(mode: .expiryText, onText: updateCandidates)
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
