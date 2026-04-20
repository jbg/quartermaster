import SwiftUI

struct MainTabView: View {
    enum Screen: Hashable { case inventory, scan, settings }

    @State private var selection: Screen = .inventory

    var body: some View {
        TabView(selection: $selection) {
            Tab("Inventory", systemImage: "basket", value: Screen.inventory) {
                NavigationStack { InventoryView() }
            }

            Tab("Scan", systemImage: "barcode.viewfinder", value: Screen.scan) {
                NavigationStack { ScanScreen() }
            }

            Tab("Settings", systemImage: "gear", value: Screen.settings) {
                NavigationStack { SettingsView() }
            }
        }
    }
}
