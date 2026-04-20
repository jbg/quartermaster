import SwiftUI

struct StockHistoryView: View {
    enum Scope {
        case household
        case batch(UUID)
    }

    @Environment(AppState.self) private var appState

    let scope: Scope
    var onChange: (() async -> Void)?

    @State private var entries: [StockEvent] = []
    @State private var nextBefore: String?
    @State private var isLoadingInitial = true
    @State private var isLoadingMore = false
    @State private var errorMessage: String?
    @State private var undoingID: UUID?

    var body: some View {
        Group {
            if isLoadingInitial {
                ProgressView("Loading…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if entries.isEmpty {
                ContentUnavailableView {
                    Label("No history yet", systemImage: "clock")
                } description: {
                    Text("Actions on stock will appear here once they happen.")
                }
            } else {
                list
            }
        }
        .navigationTitle(title)
        .task { await loadInitial() }
        .refreshable { await loadInitial() }
        .alert("Couldn't load history", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(errorMessage ?? "")
        }
    }

    private var title: String {
        switch scope {
        case .household: "Stock history"
        case .batch: "Batch history"
        }
    }

    @ViewBuilder
    private var list: some View {
        List {
            ForEach(groupedByDay, id: \.0) { (day, events) in
                Section {
                    ForEach(events) { event in
                        row(for: event)
                    }
                } header: {
                    Text(day)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .textCase(nil)
                }
            }
            if nextBefore != nil {
                HStack {
                    Spacer()
                    if isLoadingMore {
                        ProgressView()
                    } else {
                        Button("Load more") { Task { await loadMore() } }
                    }
                    Spacer()
                }
                .listRowSeparator(.hidden)
                .listRowBackground(Color.clear)
                .onAppear { Task { await loadMore() } }
            }
        }
        .listStyle(.plain)
    }

    @ViewBuilder
    private func row(for event: StockEvent) -> some View {
        StockEventRowView(event: event)
            .overlay(alignment: .trailing) {
                if undoingID == event.id {
                    ProgressView().padding(.trailing, 8)
                }
            }
            .contextMenu {
                if event.eventType == .discard {
                    Button {
                        Task { await undo(event) }
                    } label: {
                        Label("Undo discard", systemImage: "arrow.uturn.backward")
                    }
                }
            }
            .swipeActions(edge: .trailing) {
                if event.eventType == .discard {
                    Button {
                        Task { await undo(event) }
                    } label: {
                        Label("Undo", systemImage: "arrow.uturn.backward")
                    }
                    .tint(.green)
                }
            }
    }

    private var groupedByDay: [(String, [StockEvent])] {
        var groups: [(String, [StockEvent])] = []
        var current: String?
        var bucket: [StockEvent] = []
        let df = Self.dayHeader

        for event in entries {
            let label = event.createdAtDate.map(df.string(from:)) ?? "Unknown date"
            if label != current {
                if let c = current { groups.append((c, bucket)) }
                current = label
                bucket = []
            }
            bucket.append(event)
        }
        if let c = current { groups.append((c, bucket)) }
        return groups
    }

    private func loadInitial() async {
        isLoadingInitial = true
        errorMessage = nil
        nextBefore = nil
        do {
            let page = try await fetch(before: nil)
            entries = page.items
            nextBefore = page.nextBefore
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoadingInitial = false
    }

    private func loadMore() async {
        guard !isLoadingMore, let cursor = nextBefore else { return }
        isLoadingMore = true
        do {
            let page = try await fetch(before: cursor)
            entries.append(contentsOf: page.items)
            nextBefore = page.nextBefore
        } catch {
            // Keep quiet on pagination errors — user can retry via pull-to-refresh.
        }
        isLoadingMore = false
    }

    private func fetch(before cursor: String?) async throws -> StockEventListResponse {
        switch scope {
        case .household:
            try await appState.api.listStockEvents(beforeCreatedAt: cursor, limit: 50)
        case .batch(let id):
            try await appState.api.listBatchEvents(id: id, beforeCreatedAt: cursor, limit: 50)
        }
    }

    private func undo(_ event: StockEvent) async {
        undoingID = event.id
        defer { undoingID = nil }
        do {
            _ = try await appState.api.restoreStock(id: event.batchID)
            await loadInitial()
            await onChange?()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private static let dayHeader: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        return f
    }()
}
