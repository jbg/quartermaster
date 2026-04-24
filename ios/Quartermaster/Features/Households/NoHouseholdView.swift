import SwiftUI

struct NoHouseholdView: View {
  @Environment(AppState.self) private var appState

  let me: Me

  @State private var householdEntry = HouseholdEntryController()

  var body: some View {
    Form {
      Section {
        VStack(alignment: .leading, spacing: 8) {
          Text("Choose a household")
            .font(.title2.weight(.semibold))
          Text(
            "This session doesn’t have an active household yet. Join one you already belong to, redeem an invite, or create a new household to keep going."
          )
          .foregroundStyle(.secondary)
        }
        .padding(.vertical, 4)
      }

      Section("Signed in") {
        LabeledContent("Username", value: me.user.username)
        if let email = me.user.email {
          LabeledContent("Email", value: email)
        }
      }

      HouseholdEntrySections(
        controller: householdEntry,
        me: me,
        switchSectionTitle: "Switch households",
        redeemSectionTitle: "Redeem invite",
        redeemActionTitle: "Join household",
        showsCreateHousehold: true,
        onChanged: nil,
      )

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
    }
    .navigationTitle("Household")
    .task {
      applyPendingInviteContext()
    }
    .onChange(of: appState.pendingInviteContext) { _, _ in
      applyPendingInviteContext()
    }
  }

  private func applyPendingInviteContext() {
    guard let context = appState.takePendingInviteContext() else { return }
    if let inviteCode = context.inviteCode {
      householdEntry.redeemCode = inviteCode
    }
  }
}
