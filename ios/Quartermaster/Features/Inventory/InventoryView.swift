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
    @State private var pendingProduct: Product?
    @State private var resolvingDeepLink = false

    struct BatchesSheetTarget: Identifiable {
        let product: Product
        let location: Location
        /// Passed through from a deep-link so the sheet can scroll / flash
        /// the originating batch.
        var highlightBatchID: String? = nil
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
        .onChange(of: appState.pendingInventoryTarget) { _, target in
            guard let target else { return }
            Task { await resolveDeepLink(target) }
        }
        .sheet(isPresented: $showSearchSheet) {
            ProductSearchView { product in
                pendingProduct = product
            }
        }
        .sheet(item: $pendingProduct) { product in
            AddStockView(product: product) { _ in
                Task { await load() }
            }
        }
        .sheet(item: $batchesSheet) { target in
            let locationBatches = batches.filter {
                $0.product.id == target.product.id && $0.locationID == target.location.id
            }
            ProductBatchesSheet(
                product: target.product,
                location: target.location,
                allLocations: locations,
                batches: locationBatches,
                highlightBatchID: target.highlightBatchID,
            ) {
                await load()
            }
        }
    }

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
                let groups = groupedByProduct(for: location)
                Section {
                    if groups.isEmpty {
                        emptyLocationRow(location)
                    } else {
                        ForEach(groups, id: \.product.id) { group in
                            Button {
                                batchesSheet = BatchesSheetTarget(product: group.product, location: location)
                            } label: {
                                ProductRow(
                                    product: group.product,
                                    visibleBatches: group.visibleBatches,
                                    allBatches: group.allBatches,
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
        let visibleBatches: [StockBatch]
        let allBatches: [StockBatch]
    }

    private func groupedByProduct(for location: Location) -> [ProductGroup] {
        let inLocation = batches.filter { $0.locationID == location.id }
        let allByProduct = Dictionary(grouping: inLocation, by: { $0.product.id })

        var groups: [ProductGroup] = []
        for (_, all) in allByProduct {
            let visible = all.filter { filter.matches($0) }
            if visible.isEmpty { continue }

            let sortByExpiry: (StockBatch, StockBatch) -> Bool = { lhs, rhs in
                switch (lhs.expiresOnDate, rhs.expiresOnDate) {
                case let (l?, r?): return l < r
                case (nil, _?): return false
                case (_?, nil): return true
                case (nil, nil): return lhs.createdAt < rhs.createdAt
                }
            }
            let visibleSorted = visible.sorted(by: sortByExpiry)
            let allSorted = all.sorted(by: sortByExpiry)
            groups.append(ProductGroup(
                product: visibleSorted[0].product,
                visibleBatches: visibleSorted,
                allBatches: allSorted,
            ))
        }

        return groups.sorted { lhs, rhs in
            let le = lhs.visibleBatches.compactMap(\.expiresOnDate).min()
            let re = rhs.visibleBatches.compactMap(\.expiresOnDate).min()
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

    private func resolveDeepLink(_ target: InventoryTarget) async {
        guard !resolvingDeepLink else { return }
        resolvingDeepLink = true
        defer { resolvingDeepLink = false }

        // Clear the pending target first so repeated sets don't re-trigger.
        appState.pendingInventoryTarget = nil

        // Ensure inventory is loaded so the sheet's `batches` filter has data.
        if batches.isEmpty && locations.isEmpty {
            await load()
        }

        // Resolve product from the already-loaded batches first; fall back to
        // a network fetch if the product isn't represented in active stock
        // (e.g. the user is deep-linking to a depleted batch's product).
        let product: Product?
        if let fromBatches = batches.first(where: { $0.product.id == target.productID })?.product {
            product = fromBatches
        } else {
            product = try? await appState.api.getProduct(id: target.productID)
        }
        let location = locations.first(where: { $0.id == target.locationID })

        guard let product, let location else { return }
        batchesSheet = BatchesSheetTarget(
            product: product,
            location: location,
            highlightBatchID: target.highlightBatchID,
        )
    }
}
