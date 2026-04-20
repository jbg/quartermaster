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
                Text("Quartermaster • v0.1.0")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Settings")
    }
}
