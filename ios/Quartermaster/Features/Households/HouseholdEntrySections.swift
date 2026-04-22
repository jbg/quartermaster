import Observation
import SwiftUI

@MainActor
@Observable
final class HouseholdEntryController {
    var redeemCode: String = ""
    var householdNameDraft: String = ""
    var householdTimezoneID: String = TimeZone.autoupdatingCurrent.identifier

    var isSwitchingHousehold = false
    var isRedeemingInvite = false
    var isCreatingHousehold = false
    var errorMessage: String?

    func switchHousehold(
        to householdID: String,
        using appState: AppState,
        onSuccess: (@MainActor () async -> Void)? = nil
    ) async {
        guard !isSwitchingHousehold else { return }
        isSwitchingHousehold = true
        defer { isSwitchingHousehold = false }
        do {
            _ = try await appState.switchHousehold(to: householdID)
            if let onSuccess {
                await onSuccess()
            }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func redeemInvite(
        using appState: AppState,
        onSuccess: (@MainActor () async -> Void)? = nil
    ) async {
        guard !isRedeemingInvite else { return }
        isRedeemingInvite = true
        defer { isRedeemingInvite = false }
        do {
            _ = try await appState.redeemInvite(
                redeemCode.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            redeemCode = ""
            if let onSuccess {
                await onSuccess()
            }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createHousehold(
        using appState: AppState,
        onSuccess: (@MainActor () async -> Void)? = nil
    ) async {
        guard !isCreatingHousehold else { return }
        isCreatingHousehold = true
        defer { isCreatingHousehold = false }
        do {
            _ = try await appState.createHousehold(
                named: householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines),
                timezone: householdTimezoneID
            )
            householdNameDraft = ""
            if let onSuccess {
                await onSuccess()
            }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct HouseholdEntrySections: View {
    @Environment(AppState.self) private var appState
    @Bindable var controller: HouseholdEntryController

    let me: Me
    let switchSectionTitle: String
    let redeemSectionTitle: String
    let redeemActionTitle: String
    let showsCreateHousehold: Bool
    let onChanged: (@MainActor () async -> Void)?

    var body: some View {
        Group {
            if !me.households.isEmpty {
                Section(switchSectionTitle) {
                    ForEach(me.households) { membership in
                        Button {
                            Task {
                                await controller.switchHousehold(
                                    to: membership.household.id,
                                    using: appState,
                                    onSuccess: onChanged,
                                )
                            }
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(membership.household.name)
                                    Text(membership.role.displayName)
                                        .font(.footnote)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                if me.householdId == membership.household.id {
                                    Image(systemName: "checkmark.circle.fill")
                                        .foregroundStyle(.tint)
                                } else if controller.isSwitchingHousehold {
                                    ProgressView()
                                        .controlSize(.small)
                                }
                            }
                        }
                        .disabled(
                            controller.isSwitchingHousehold
                                || me.householdId == membership.household.id
                        )
                    }
                }
            }

            Section(redeemSectionTitle) {
                TextField("Invite code", text: $controller.redeemCode)
                    .textInputAutocapitalization(.characters)
                    .autocorrectionDisabled()
                Button {
                    Task {
                        await controller.redeemInvite(
                            using: appState,
                            onSuccess: onChanged,
                        )
                    }
                } label: {
                    if controller.isRedeemingInvite {
                        ProgressView()
                    } else {
                        Text(redeemActionTitle)
                    }
                }
                .disabled(
                    controller.redeemCode
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                        .isEmpty || controller.isRedeemingInvite
                )
            }

            if showsCreateHousehold {
                Section("Create household") {
                    TextField("Household name", text: $controller.householdNameDraft)
                    Picker("Timezone", selection: $controller.householdTimezoneID) {
                        ForEach(TimeZone.knownTimeZoneIdentifiers, id: \.self) { identifier in
                            Text(identifier).tag(identifier)
                        }
                    }
                    Button {
                        Task {
                            await controller.createHousehold(
                                using: appState,
                                onSuccess: onChanged,
                            )
                        }
                    } label: {
                        if controller.isCreatingHousehold {
                            ProgressView()
                        } else {
                            Text("Create household")
                        }
                    }
                    .disabled(
                        controller.householdNameDraft
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                            .isEmpty || controller.isCreatingHousehold
                    )
                }
            }
        }
        .alert("Couldn't complete that action", isPresented: Binding(
            get: { controller.errorMessage != nil },
            set: { if !$0 { controller.errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(controller.errorMessage ?? "")
        }
    }
}
