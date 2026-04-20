import SwiftUI

struct SettingsView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Form {
            if case .authenticated(let me) = appState.phase {
                Section("Signed in") {
                    LabeledContent("Username", value: me.user.username)
                    if let email = me.user.email {
                        LabeledContent("Email", value: email)
                    }
                    if let household = me.household {
                        LabeledContent("Household", value: household.name)
                    }
                }
            }

            Section("Inventory") {
                NavigationLink {
                    StockHistoryView(scope: .household)
                } label: {
                    Label("Stock history", systemImage: "clock.arrow.circlepath")
                }
            }

            Section("Server") {
                LabeledContent("URL", value: appState.serverURL.absoluteString)
            }

            Section {
                Button(role: .destructive) {
                    Task { await appState.logout() }
                } label: {
                    Text("Sign out")
                }
            }

            Section {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Product data attribution")
                        .font(.footnote.weight(.semibold))
                    Text("Barcode lookups use [Open Food Facts](https://world.openfoodfacts.org), an open database available under the [Open Database Licence (ODbL)](https://opendatacommons.org/licenses/odbl/1-0/).")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 2)
            } header: {
                Text("About")
            }

            Section {
                Text("Quartermaster • v0.1.0")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Settings")
    }
}
