import SwiftUI

struct NoHouseholdView: View {
    @Environment(AppState.self) private var appState

    let me: Me

    @State private var redeemCode: String = ""
    @State private var householdNameDraft: String = ""
    @State private var isSwitchingHousehold = false
    @State private var isRedeemingInvite = false
    @State private var isCreatingHousehold = false
    @State private var errorMessage: String?

    var body: some View {
        Form {
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Choose a household")
                        .font(.title2.weight(.semibold))
                    Text("This session doesn’t have an active household yet. Join one you already belong to, redeem an invite, or create a new household to keep going.")
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

            if !me.households.isEmpty {
                Section("Switch households") {
                    ForEach(me.households) { membership in
                        Button {
                            Task { await switchHousehold(to: membership.household.id) }
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(membership.household.name)
                                    Text(membership.role.displayName)
                                        .font(.footnote)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                if isSwitchingHousehold {
                                    ProgressView()
                                        .controlSize(.small)
                                }
                            }
                        }
                        .disabled(isSwitchingHousehold)
                    }
                }
            }

            Section("Redeem invite") {
                TextField("Invite code", text: $redeemCode)
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                Button {
                    Task { await redeemInvite() }
                } label: {
                    if isRedeemingInvite {
                        ProgressView()
                    } else {
                        Text("Join household")
                    }
                }
                .disabled(redeemCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isRedeemingInvite)
            }

            Section("Create household") {
                TextField("Household name", text: $householdNameDraft)
                Button {
                    Task { await createHousehold() }
                } label: {
                    if isCreatingHousehold {
                        ProgressView()
                    } else {
                        Text("Create household")
                    }
                }
                .disabled(householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isCreatingHousehold)
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
        }
        .navigationTitle("Household")
        .task {
            applyPendingInviteContext()
        }
        .onChange(of: appState.pendingInviteContext) { _, _ in
            applyPendingInviteContext()
        }
        .alert("Couldn't complete that action", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(errorMessage ?? "")
        }
    }

    private func switchHousehold(to householdID: String) async {
        isSwitchingHousehold = true
        defer { isSwitchingHousehold = false }
        do {
            let updatedMe = try await appState.api.switchHousehold(householdID: householdID)
            appState.applyAuthenticated(updatedMe)
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func redeemInvite() async {
        isRedeemingInvite = true
        defer { isRedeemingInvite = false }
        do {
            try await appState.api.redeemInvite(code: redeemCode.trimmingCharacters(in: .whitespacesAndNewlines))
            redeemCode = ""
            await appState.refreshMe()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func createHousehold() async {
        isCreatingHousehold = true
        defer { isCreatingHousehold = false }
        do {
            let updatedMe = try await appState.api.createHousehold(
                name: householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            householdNameDraft = ""
            appState.applyAuthenticated(updatedMe)
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func applyPendingInviteContext() {
        guard let context = appState.takePendingInviteContext() else { return }
        if let inviteCode = context.inviteCode {
            redeemCode = inviteCode
        }
    }
}
