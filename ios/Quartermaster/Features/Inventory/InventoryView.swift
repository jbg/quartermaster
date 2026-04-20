import SwiftUI

struct InventoryView: View {
    @Environment(AppState.self) private var appState

    enum LoadState {
        case idle
        case loading
        case loaded([Location])
        case failed(String)
    }

    @State private var state: LoadState = .idle

    var body: some View {
        Group {
            switch state {
            case .idle, .loading:
                ProgressView("Loading…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .loaded(let locations):
                loadedView(locations)
            case .failed(let message):
                ContentUnavailableView {
                    Label("Couldn't load inventory", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(message)
                } actions: {
                    Button("Try again") { Task { await load() } }
                        .buttonStyle(.borderedProminent)
                }
            }
        }
        .navigationTitle("Inventory")
        .task { await load() }
        .refreshable { await load() }
    }

    @ViewBuilder
    private func loadedView(_ locations: [Location]) -> some View {
        List {
            ForEach(locations) { loc in
                Section {
                    ContentUnavailableView(
                        "Nothing in \(loc.name) yet",
                        systemImage: iconName(for: loc.kind),
                        description: Text("Scan a barcode or tap Add to get started."),
                    )
                    .padding(.vertical, 20)
                    .frame(maxWidth: .infinity)
                } header: {
                    Label(loc.name, systemImage: iconName(for: loc.kind))
                        .font(.headline)
                        .textCase(nil)
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    private func iconName(for kind: String) -> String {
        switch kind {
        case "pantry": "cabinet"
        case "fridge": "refrigerator"
        case "freezer": "snowflake"
        default: "archivebox"
        }
    }

    private func load() async {
        state = .loading
        do {
            let locations = try await appState.api.locations()
            state = .loaded(locations)
        } catch {
            if let apiError = error as? APIError {
                state = .failed(apiError.userFacingMessage)
            } else {
                state = .failed(error.localizedDescription)
            }
        }
    }
}
