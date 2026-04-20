import SwiftUI

struct ScanPlaceholderView: View {
    var body: some View {
        ContentUnavailableView {
            Label("Barcode scanning", systemImage: "barcode.viewfinder")
        } description: {
            Text("Arriving in the next slice — product search + scan-to-add stock.")
        }
        .navigationTitle("Scan")
    }
}
