import SwiftUI

struct ProductSearchView: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss

    var onPick: (Product) -> Void

    @State private var query: String = ""
    @State private var results: [Product] = []
    @State private var isSearching = false
    @State private var searchTask: Task<Void, Never>?
    @State private var showManualCreate = false

    var body: some View {
        NavigationStack {
            List {
                if query.count < 2 {
                    Section {
                        Text("Type at least two characters to search.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                } else if results.isEmpty && !isSearching {
                    Section {
                        ContentUnavailableView {
                            Label("No matches", systemImage: "magnifyingglass")
                        } description: {
                            Text("Try a different search, or create a product manually.")
                        }
                    }
                }
                ForEach(results) { product in
                    Button {
                        onPick(product)
                        dismiss()
                    } label: {
                        ProductListRow(product: product)
                    }
                    .buttonStyle(.plain)
                }
                Section {
                    Button {
                        showManualCreate = true
                    } label: {
                        Label("Create a product manually", systemImage: "plus.circle")
                    }
                }
            }
            .searchable(text: $query, prompt: "Search products")
            .navigationTitle("Add stock")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
            .onChange(of: query) { _, newValue in
                searchTask?.cancel()
                if newValue.count < 2 {
                    results = []
                    return
                }
                searchTask = Task {
                    try? await Task.sleep(for: .milliseconds(250))
                    if Task.isCancelled { return }
                    await performSearch(newValue)
                }
            }
            .sheet(isPresented: $showManualCreate) {
                ManualProductForm(prefillBarcode: nil) { created in
                    onPick(created)
                    dismiss()
                }
            }
        }
    }

    private func performSearch(_ q: String) async {
        isSearching = true
        let fetched = (try? await appState.api.searchProducts(query: q)) ?? []
        if !Task.isCancelled {
            results = fetched
        }
        isSearching = false
    }
}

struct ProductListRow: View {
    let product: Product

    var body: some View {
        HStack(spacing: 12) {
            if let url = product.imageURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFit()
                    default:
                        Color.secondary.opacity(0.12)
                    }
                }
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            } else {
                Image(systemName: "shippingbox")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .frame(width: 44, height: 44)
                    .background(Color.secondary.opacity(0.1))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(product.displayTitle).lineLimit(2)
                HStack(spacing: 4) {
                    Text(product.family.displayName)
                    if product.source == "manual" {
                        Text("· Manual")
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
    }
}
