import SwiftUI

struct StockHistoryView: View {
  enum Scope {
    case household
    case batch(String)
  }

  @Environment(AppState.self) private var appState

  let scope: Scope
  var onChange: (() async -> Void)?

  @State private var entries: [StockEvent] = []
  @State private var nextBefore: String?
  @State private var nextBeforeID: String?
  @State private var isLoadingInitial = true
  @State private var isLoadingMore = false
  @State private var errorMessage: String?
  @State private var expandedGroups: Set<String> = []
  @State private var selectionMode = false
  @State private var selected: Set<String> = []
  @State private var isRestoring = false

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
          .safeAreaInset(edge: .bottom) {
            if selectionMode {
              bulkRestoreBar
            }
          }
      }
    }
    .accessibilityIdentifier("stock-history.root")
    .navigationTitle(title)
    .navigationDestination(for: String.self) { batchID in
      BatchDetailView(batchID: batchID)
    }
    .toolbar {
      if !entries.isEmpty {
        ToolbarItem(placement: .topBarTrailing) {
          Button(selectionMode ? "Cancel" : "Select") {
            withAnimation {
              selectionMode.toggle()
              if !selectionMode { selected.removeAll() }
            }
          }
        }
      }
    }
    .task { await loadInitial() }
    .refreshable { await loadInitial() }
    .alert(
      "Couldn't load history",
      isPresented: Binding(
        get: { errorMessage != nil },
        set: { if !$0 { errorMessage = nil } }
      )
    ) {
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
      ForEach(groupedByDay, id: \.0) { (day, groups) in
        Section {
          ForEach(groups) { group in
            rowFor(group)
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
  private func rowFor(_ group: DisplayGroup) -> some View {
    switch group {
    case .single(let event):
      singleRow(event)
    case .consumeGroup(let id, let events):
      // Consume groups are not selectable — they represent a single
      // action that already landed atomically. Select mode simply
      // greys them out.
      ConsumeGroupRow(
        requestID: id,
        events: events,
        preferredUnit: events.first?.product.preferredUnit ?? "",
        units: appState.units,
        expandedGroups: $expandedGroups,
      )
      .opacity(selectionMode ? 0.5 : 1)
    }
  }

  @ViewBuilder
  private func singleRow(_ event: StockEvent) -> some View {
    if selectionMode {
      Button {
        toggleSelection(event)
      } label: {
        HStack {
          StockEventRowView(event: event)
            .opacity(event.eventType == .discard ? 1 : 0.4)
          Spacer()
          Image(systemName: selected.contains(event.id) ? "checkmark.circle.fill" : "circle")
            .font(.title3)
            .foregroundStyle(event.eventType == .discard ? Color.accentColor : .secondary)
        }
      }
      .buttonStyle(.plain)
      .disabled(event.eventType != .discard)
    } else {
      NavigationLink(value: event.batchID) {
        StockEventRowView(event: event)
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
  }

  @ViewBuilder
  private var bulkRestoreBar: some View {
    HStack {
      Text("\(selected.count) selected")
        .font(.footnote)
        .foregroundStyle(.secondary)
      Spacer()
      Button {
        Task { await bulkRestore() }
      } label: {
        if isRestoring {
          ProgressView()
        } else {
          Text("Undo \(selected.count)").fontWeight(.semibold)
        }
      }
      .buttonStyle(.borderedProminent)
      .disabled(selected.isEmpty || isRestoring)
    }
    .padding()
    .background(.thinMaterial)
  }

  // MARK: - Grouping

  enum DisplayGroup: Identifiable {
    case single(StockEvent)
    case consumeGroup(String, [StockEvent])

    var id: String {
      switch self {
      case .single(let e): e.id
      case .consumeGroup(let rid, _): "group-\(rid)"
      }
    }

    var timestamp: String {
      switch self {
      case .single(let e): e.createdAt
      case .consumeGroup(_, let events): events.first?.createdAt ?? ""
      }
    }
  }

  private var displayGroups: [DisplayGroup] {
    var groups: [DisplayGroup] = []
    var i = 0
    while i < entries.count {
      let event = entries[i]
      if event.eventType == .consume, let rid = event.consumeRequestID {
        var batch: [StockEvent] = [event]
        var j = i + 1
        while j < entries.count,
          entries[j].eventType == .consume,
          entries[j].consumeRequestID == rid
        {
          batch.append(entries[j])
          j += 1
        }
        if batch.count >= 2 {
          groups.append(.consumeGroup(rid, batch))
          i = j
          continue
        }
      }
      groups.append(.single(event))
      i += 1
    }
    return groups
  }

  private var groupedByDay: [(String, [DisplayGroup])] {
    var out: [(String, [DisplayGroup])] = []
    var current: String?
    var bucket: [DisplayGroup] = []
    let df = Self.dayHeader

    for group in displayGroups {
      let date: Date? = {
        let iso = group.timestamp
        return Self.iso.date(from: iso)
      }()
      let label = date.map(df.string(from:)) ?? "Unknown date"
      if label != current {
        if let c = current { out.append((c, bucket)) }
        current = label
        bucket = []
      }
      bucket.append(group)
    }
    if let c = current { out.append((c, bucket)) }
    return out
  }

  // MARK: - Actions

  private func toggleSelection(_ event: StockEvent) {
    guard event.eventType == .discard else { return }
    if selected.contains(event.id) {
      selected.remove(event.id)
    } else {
      selected.insert(event.id)
    }
  }

  private func loadInitial() async {
    isLoadingInitial = true
    errorMessage = nil
    nextBefore = nil
    nextBeforeID = nil
    do {
      let page = try await fetch(beforeCreatedAt: nil, beforeID: nil)
      entries = page.items
      nextBefore = page.nextBefore
      nextBeforeID = page.nextBeforeID
    } catch let err as APIError {
      if case .server(status: 403, _) = err, isHouseholdScope {
        switch await appState.resolveHouseholdScopedForbidden() {
        case .retry:
          await loadInitial()
          return
        case .fallbackToNoHousehold:
          entries = []
          isLoadingInitial = false
          return
        case .failed(let message):
          errorMessage = message
          return
        }
      }
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
    isLoadingInitial = false
  }

  private func loadMore() async {
    guard !isLoadingMore, let cursorTs = nextBefore else { return }
    isLoadingMore = true
    do {
      let page = try await fetch(beforeCreatedAt: cursorTs, beforeID: nextBeforeID)
      entries.append(contentsOf: page.items)
      nextBefore = page.nextBefore
      nextBeforeID = page.nextBeforeID
    } catch {
      // Keep quiet on pagination errors — user can retry via pull-to-refresh.
    }
    isLoadingMore = false
  }

  private func fetch(beforeCreatedAt: String?, beforeID: String?) async throws
    -> StockEventListResponse
  {
    switch scope {
    case .household:
      try await appState.api.listStockEvents(
        beforeCreatedAt: beforeCreatedAt,
        beforeID: beforeID,
        limit: 50,
      )
    case .batch(let id):
      try await appState.api.listBatchEvents(
        id: id,
        beforeCreatedAt: beforeCreatedAt,
        beforeID: beforeID,
        limit: 50,
      )
    }
  }

  private var isHouseholdScope: Bool {
    if case .household = scope {
      return true
    }
    return false
  }

  private func undo(_ event: StockEvent) async {
    do {
      _ = try await appState.api.restoreStock(id: event.batchID)
      await appState.refreshRemindersAfterInventoryMutation()
      await loadInitial()
      await onChange?()
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func bulkRestore() async {
    guard !selected.isEmpty else { return }
    isRestoring = true
    defer { isRestoring = false }
    let batchIDs: [String] = selected.compactMap { eventID in
      entries.first(where: { $0.id == eventID })?.batchID
    }
    guard !batchIDs.isEmpty else { return }
    do {
      _ = try await appState.api.restoreManyStock(ids: batchIDs)
      await appState.refreshRemindersAfterInventoryMutation()
      selectionMode = false
      selected.removeAll()
      await loadInitial()
      await onChange?()
    } catch let err as APIError {
      errorMessage = bulkRestoreErrorMessage(for: err)
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  /// When `POST /stock/restore-many` rolls back, the server returns a
  /// 409 whose body names the specific `unrestorable_ids`. Fold that into
  /// a user-facing message that points at product names rather than
  /// opaque UUIDs when we can resolve them.
  private func bulkRestoreErrorMessage(for error: APIError) -> String {
    if case .server(status: 409, let body) = error,
      let body,
      body.code == "batch_not_restorable",
      let ids = body.unrestorableIds, !ids.isEmpty
    {
      let names: [String] = ids.compactMap { id in
        entries.first(where: { $0.batchID == id })?.product.displayTitle
      }
      let count = ids.count
      let subject = count == 1 ? "1 batch" : "\(count) batches"
      if names.isEmpty {
        return "\(subject) couldn't be undone — they may already have been restored."
      }
      // De-dup while preserving first-seen order so a batch touched
      // by two events doesn't appear twice.
      var seen = Set<String>()
      let unique = names.filter { seen.insert($0).inserted }
      return
        "Couldn't undo: \(unique.joined(separator: ", ")). They may already have been restored."
    }
    return error.userFacingMessage
  }

  private static let dayHeader: DateFormatter = {
    let f = DateFormatter()
    f.dateStyle = .medium
    f.timeStyle = .none
    return f
  }()

  nonisolated(unsafe) private static let iso: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return f
  }()
}
