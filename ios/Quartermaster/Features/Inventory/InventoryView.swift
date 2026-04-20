import SwiftUI

struct InventoryView: View {
    @Environment(AppState.self) private var appState

    @State private var locations: [Location] = []
    @State private var batches: [StockBatch] = []
    @State private var isLoading: Bool = true
    @State private var loadError: String?
    @State private var filter: InventoryFilter = .all

    @State private var showSearchSheet = false
    @State private var batchesSheet: BatchesSheetTarget?

    struct BatchesSheetTarget: Identifiable {
        let product: Product
        let location: Location
        var id: String { "\(product.id)-\(location.id)" }
    }

    var body: some View {
        Group {
            if isLoading && batches.isEmpty && locations.isEmpty {
                ProgressView("Loading…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let msg = loadError {
                ContentUnavailableView {
                    Label("Couldn't load inventory", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(msg)
                } actions: {
                    Button("Try again") { Task { await load() } }
                        .buttonStyle(.borderedProminent)
                }
            } else {
                loadedContent
            }
        }
        .navigationTitle("Inventory")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showSearchSheet = true
                } label: {
                    Label("Add stock", systemImage: "plus")
                }
            }
        }
        .task { await load() }
        .refreshable { await load() }
        .sheet(isPresented: $showSearchSheet) {
            ProductSearchView { product in
                // Present AddStockView after product is picked.
                // Using nested sheet via a small coordinator state.
                pendingProduct = product
            }
        }
        .sheet(item: $pendingProduct) { product in
            AddStockView(product: product) { _ in
                Task { await load() }
            }
        }
        .sheet(item: $batchesSheet) { target in
            ProductBatchesSheet(
                product: target.product,
                location: target.location,
                batches: batches.filter { $0.product.id == target.product.id && $0.locationID == target.location.id },
            ) {
                await load()
            }
        }
    }

    @State private var pendingProduct: Product?

    @ViewBuilder
    private var loadedContent: some View {
        List {
            Section {
                Picker("Filter", selection: $filter.animation(.easeInOut(duration: 0.15))) {
                    ForEach(InventoryFilter.allCases) { f in
                        Text(f.rawValue).tag(f)
                    }
                }
                .pickerStyle(.segmented)
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
            }

            ForEach(locations) { location in
                let byProduct = groupedByProduct(for: location)
                Section {
                    if byProduct.isEmpty {
                        emptyLocationRow(location)
                    } else {
                        ForEach(byProduct, id: \.product.id) { group in
                            Button {
                                batchesSheet = BatchesSheetTarget(product: group.product, location: location)
                            } label: {
                                ProductRow(
                                    product: group.product,
                                    batches: group.batches,
                                    units: appState.units,
                                )
                            }
                            .buttonStyle(.plain)
                        }
                    }
                } header: {
                    Label(location.name, systemImage: icon(for: location.kind))
                        .font(.subheadline.weight(.semibold))
                        .textCase(nil)
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    private struct ProductGroup {
        let product: Product
        let batches: [StockBatch]
    }

    private func groupedByProduct(for location: Location) -> [ProductGroup] {
        let forLocation = batches.filter { $0.locationID == location.id && filter.matches($0) }
        let dict = Dictionary(grouping: forLocation, by: { $0.product.id })
        return dict.values
            .compactMap { group -> ProductGroup? in
                guard let any = group.first else { return nil }
                let sorted = group.sorted { lhs, rhs in
                    switch (lhs.expiresOnDate, rhs.expiresOnDate) {
                    case let (l?, r?): return l < r
                    case (nil, _?): return false
                    case (_?, nil): return true
                    case (nil, nil): return lhs.createdAt < rhs.createdAt
                    }
                }
                return ProductGroup(product: any.product, batches: sorted)
            }
            .sorted { lhs, rhs in
                let le = lhs.batches.compactMap(\.expiresOnDate).min()
                let re = rhs.batches.compactMap(\.expiresOnDate).min()
                switch (le, re) {
                case let (l?, r?): return l < r
                case (nil, _?): return false
                case (_?, nil): return true
                case (nil, nil): return lhs.product.name < rhs.product.name
                }
            }
    }

    @ViewBuilder
    private func emptyLocationRow(_ location: Location) -> some View {
        switch filter {
        case .all:
            ContentUnavailableView(
                "Nothing in \(location.name) yet",
                systemImage: icon(for: location.kind),
                description: Text("Scan a barcode or tap + to add stock."),
            )
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity)
        case .expiringSoon:
            Text("Nothing expiring in the next week here.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .expired:
            Text("Nothing expired here.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func icon(for kind: String) -> String {
        switch kind {
        case "pantry": "cabinet"
        case "fridge": "refrigerator"
        case "freezer": "snowflake"
        default: "archivebox"
        }
    }

    private func load() async {
        isLoading = true
        loadError = nil
        async let locs = appState.api.locations()
        async let stock = appState.api.listStock()
        do {
            let (l, s) = try await (locs, stock)
            locations = l.sorted { $0.sortOrder < $1.sortOrder }
            batches = s
        } catch let err as APIError {
            loadError = err.userFacingMessage
        } catch {
            loadError = error.localizedDescription
        }
        isLoading = false
    }
}
