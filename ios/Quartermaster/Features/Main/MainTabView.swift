import SwiftUI

struct MainTabView: View {
  @Environment(AppState.self) private var appState
  enum Screen: Hashable { case inventory, reminders, scan, settings }

  @State private var selection: Screen = .inventory

  var body: some View {
    TabView(selection: $selection) {
      Tab("Inventory", systemImage: "basket", value: Screen.inventory) {
        NavigationStack { InventoryView() }
      }

      Tab("Reminders", systemImage: "bell", value: Screen.reminders) {
        NavigationStack { ReminderInboxView() }
      }

      Tab("Scan", systemImage: "barcode.viewfinder", value: Screen.scan) {
        NavigationStack { ScanScreen() }
      }

      Tab("Settings", systemImage: "gear", value: Screen.settings) {
        NavigationStack { SettingsView() }
      }
    }
    .onChange(of: appState.pendingInventoryTarget) { _, target in
      // Deep-link out of history / other tabs into Inventory. The
      // Inventory view itself observes the same value and presents
      // the batches sheet.
      if target != nil {
        selection = .inventory
      }
    }
    .onChange(of: appState.pendingInviteContext) { _, target in
      if target != nil {
        selection = .settings
      }
    }
  }
}
