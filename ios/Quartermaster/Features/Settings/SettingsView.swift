import SwiftUI

struct SettingsView: View {
    @Environment(AppState.self) private var appState

    @State private var household: HouseholdDetail?
    @State private var members: [Member] = []
    @State private var invites: [Invite] = []
    @State private var locations: [Location] = []

    @State private var householdNameDraft: String = ""
    @State private var redeemCode: String = ""
    @State private var newInviteMaxUses: Int = 1
    @State private var newInviteRole: MembershipRole = .member
    @State private var newInviteExpiry: Date = Calendar.current.date(byAdding: .day, value: 7, to: .now) ?? .now

    @State private var showLocationEditor = false
    @State private var editingLocation: Location?

    @State private var isLoading = true
    @State private var isSavingHousehold = false
    @State private var isRedeemingInvite = false
    @State private var isCreatingInvite = false
    @State private var errorMessage: String?
    @State private var showRenameConfirmation = false
    @State private var invitePendingRevocation: Invite?
    @State private var memberPendingRemoval: Member?
    @State private var locationPendingDeletion: Location?

    var body: some View {
        Form {
            if let me {
                Section("Signed in") {
                    LabeledContent("Username", value: me.user.username)
                    if let email = me.user.email {
                        LabeledContent("Email", value: email)
                    }
                    if let household {
                        LabeledContent("Household", value: household.name)
                    }
                    LabeledContent("Role", value: currentRole?.displayName ?? "Member")
                }
            }

            Section("Inventory") {
                NavigationLink {
                    StockHistoryView(scope: .household)
                } label: {
                    Label("Stock history", systemImage: "clock.arrow.circlepath")
                }
            }

            if let household {
                Section("Household") {
                    if isAdmin {
                        TextField("Household name", text: $householdNameDraft)
                        Button {
                            showRenameConfirmation = true
                        } label: {
                            if isSavingHousehold {
                                ProgressView()
                            } else {
                                Text("Save name")
                            }
                        }
                        .disabled(householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isSavingHousehold)
                    } else {
                        LabeledContent("Name", value: household.name)
                    }
                }

                Section("Join another household") {
                    TextField("Invite code", text: $redeemCode)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                    Button {
                        Task { await redeemInvite() }
                    } label: {
                        if isRedeemingInvite {
                            ProgressView()
                        } else {
                            Text("Redeem invite")
                        }
                    }
                    .disabled(redeemCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isRedeemingInvite)
                }
            }

            if isAdmin {
                Section("Create invite") {
                    Picker("Role", selection: $newInviteRole) {
                        Text("Member").tag(MembershipRole.member)
                        Text("Admin").tag(MembershipRole.admin)
                    }
                    Stepper("Uses: \(newInviteMaxUses)", value: $newInviteMaxUses, in: 1...99)
                    DatePicker("Expires", selection: $newInviteExpiry, in: .now..., displayedComponents: [.date, .hourAndMinute])
                    Button {
                        Task { await createInvite() }
                    } label: {
                        if isCreatingInvite {
                            ProgressView()
                        } else {
                            Text("Create invite")
                        }
                    }
                    .disabled(isCreatingInvite)
                }

                Section("Invites") {
                    if invites.isEmpty {
                        Text("No active invites.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(invites) { invite in
                            VStack(alignment: .leading, spacing: 8) {
                                HStack {
                                    Text(invite.code)
                                        .font(.headline.monospaced())
                                    Spacer()
                                    ShareLink(
                                        item: inviteShareText(invite),
                                        preview: SharePreview("Quartermaster Invite", image: Image(systemName: "person.2.badge.plus"))
                                    ) {
                                        Image(systemName: "square.and.arrow.up")
                                    }
                                    Button(role: .destructive) {
                                        invitePendingRevocation = invite
                                    } label: {
                                        Image(systemName: "trash")
                                    }
                                }
                                Text("\(invite.roleGranted.displayName) • \(invite.useCount)/\(invite.maxUses) uses • expires \(invite.expiresAt)")
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }

            Section("Members") {
                if members.isEmpty {
                    Text("No members found.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(members) { member in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(member.user.username)
                                if let email = member.user.email {
                                    Text(email)
                                        .font(.footnote)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            Spacer()
                            Text(member.role.displayName)
                                .font(.footnote.weight(.semibold))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(.secondary.opacity(0.12), in: Capsule())
                            if isAdmin && member.user.id != me?.user.id {
                                Button(role: .destructive) {
                                    memberPendingRemoval = member
                                } label: {
                                    Image(systemName: "person.badge.minus")
                                }
                            }
                        }
                    }
                }
            }

            Section("Locations") {
                if locations.isEmpty {
                    Text("No locations yet.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(Array(locations.enumerated()), id: \.element.id) { index, location in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(location.name)
                                Text(location.kind.capitalized)
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if isAdmin {
                                Button {
                                    Task { await moveLocation(from: index, direction: -1) }
                                } label: {
                                    Image(systemName: "arrow.up")
                                }
                                .disabled(index == 0)
                                Button {
                                    Task { await moveLocation(from: index, direction: 1) }
                                } label: {
                                    Image(systemName: "arrow.down")
                                }
                                .disabled(index == locations.count - 1)
                                Button {
                                    editingLocation = location
                                    showLocationEditor = true
                                } label: {
                                    Image(systemName: "pencil")
                                }
                                Button(role: .destructive) {
                                    locationPendingDeletion = location
                                } label: {
                                    Image(systemName: "trash")
                                }
                            }
                        }
                    }
                }
                if isAdmin {
                    Button {
                        editingLocation = nil
                        showLocationEditor = true
                    } label: {
                        Label("Add location", systemImage: "plus")
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
        .overlay {
            if isLoading {
                ProgressView("Loading…")
            }
        }
        .task {
            applyPendingInviteContext()
            await load()
        }
        .refreshable { await load() }
        .onChange(of: appState.pendingInviteContext) { _, _ in
            applyPendingInviteContext()
        }
        .sheet(isPresented: $showLocationEditor) {
            NavigationStack {
                LocationEditorView(location: editingLocation) { name, kind in
                    await saveLocation(name: name, kind: kind)
                }
            }
        }
        .alert("Couldn't complete that action", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(errorMessage ?? "")
        }
        .confirmationDialog(
            "Save household name?",
            isPresented: $showRenameConfirmation,
            titleVisibility: .visible
        ) {
            Button("Save") {
                Task { await saveHouseholdName() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Rename the household to \(householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines))?")
        }
        .confirmationDialog(
            "Revoke invite?",
            isPresented: Binding(
                get: { invitePendingRevocation != nil },
                set: { if !$0 { invitePendingRevocation = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Revoke Invite", role: .destructive) {
                guard let invitePendingRevocation else { return }
                Task { await revokeInvite(invitePendingRevocation) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if let invitePendingRevocation {
                Text("Invite \(invitePendingRevocation.code) will stop working immediately.")
            }
        }
        .confirmationDialog(
            "Remove member?",
            isPresented: Binding(
                get: { memberPendingRemoval != nil },
                set: { if !$0 { memberPendingRemoval = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Remove Member", role: .destructive) {
                guard let memberPendingRemoval else { return }
                Task { await removeMember(memberPendingRemoval) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if let memberPendingRemoval {
                Text("\(memberPendingRemoval.user.username) will lose access to this household.")
            }
        }
        .confirmationDialog(
            "Delete location?",
            isPresented: Binding(
                get: { locationPendingDeletion != nil },
                set: { if !$0 { locationPendingDeletion = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Delete Location", role: .destructive) {
                guard let locationPendingDeletion else { return }
                Task { await deleteLocation(locationPendingDeletion) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if let locationPendingDeletion {
                Text("\(locationPendingDeletion.name) will be removed if it has no active stock.")
            }
        }
    }

    private var me: Me? {
        if case .authenticated(let me) = appState.phase {
            return me
        }
        return nil
    }

    private var currentRole: MembershipRole? {
        guard let me else { return nil }
        return members.first(where: { $0.user.id == me.user.id })?.role
    }

    private var isAdmin: Bool {
        currentRole == .admin
    }

    private func load() async {
        guard me != nil else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            async let householdReq = appState.api.currentHousehold()
            async let membersReq = appState.api.householdMembers()
            async let locationsReq = appState.api.locations()
            let (household, members, locations) = try await (householdReq, membersReq, locationsReq)
            self.household = household
            self.householdNameDraft = household.name
            self.members = members
            self.locations = locations.sorted { $0.sortOrder < $1.sortOrder }
            if members.first(where: { $0.user.id == me?.user.id })?.role == .admin {
                invites = try await appState.api.householdInvites()
            } else {
                invites = []
            }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func saveHouseholdName() async {
        isSavingHousehold = true
        defer { isSavingHousehold = false }
        do {
            let updated = try await appState.api.updateCurrentHousehold(
                name: householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            household = updated
            await appState.refreshMe()
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
            await load()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func createInvite() async {
        isCreatingInvite = true
        defer { isCreatingInvite = false }
        do {
            _ = try await appState.api.createInvite(
                expiresAt: Self.rfc3339.string(from: newInviteExpiry),
                maxUses: newInviteMaxUses,
                role: newInviteRole
            )
            invites = try await appState.api.householdInvites()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func revokeInvite(_ invite: Invite) async {
        defer { invitePendingRevocation = nil }
        do {
            try await appState.api.revokeInvite(id: invite.id)
            invites.removeAll { $0.id == invite.id }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func removeMember(_ member: Member) async {
        defer { memberPendingRemoval = nil }
        do {
            try await appState.api.removeHouseholdMember(userID: member.user.id)
            members.removeAll { $0.user.id == member.user.id }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func saveLocation(name: String, kind: String) async {
        do {
            if let editingLocation {
                _ = try await appState.api.updateLocation(
                    id: editingLocation.id,
                    name: name,
                    kind: kind,
                    sortOrder: Int(editingLocation.sortOrder)
                )
            } else {
                _ = try await appState.api.createLocation(name: name, kind: kind)
            }
            showLocationEditor = false
            self.editingLocation = nil
            locations = try await appState.api.locations()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func moveLocation(from index: Int, direction: Int) async {
        let target = index + direction
        guard locations.indices.contains(target) else { return }
        var reordered = locations
        reordered.swapAt(index, target)
        do {
            for (idx, location) in reordered.enumerated() {
                _ = try await appState.api.updateLocation(
                    id: location.id,
                    name: location.name,
                    kind: location.kind,
                    sortOrder: idx
                )
            }
            locations = try await appState.api.locations()
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func deleteLocation(_ location: Location) async {
        defer { locationPendingDeletion = nil }
        do {
            try await appState.api.deleteLocation(id: location.id)
            locations.removeAll { $0.id == location.id }
        } catch let err as APIError {
            errorMessage = err.userFacingMessage
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func inviteShareText(_ invite: Invite) -> String {
        let householdName = household?.name ?? "your household"
        let link = inviteJoinURL(invite)?.absoluteString ?? appState.serverURL.absoluteString
        return """
        Join \(householdName) in Quartermaster.

        Open link: \(link)
        Server URL: \(appState.serverURL.absoluteString)
        Invite code: \(invite.code)

        Open Quartermaster, go to Settings, and choose “Redeem invite”.
        """
    }

    private func inviteJoinURL(_ invite: Invite) -> URL? {
        var components = URLComponents(url: appState.serverURL, resolvingAgainstBaseURL: false)
        components?.path = "/join"
        components?.queryItems = [
            URLQueryItem(name: "invite", value: invite.code),
            URLQueryItem(name: "server", value: appState.serverURL.absoluteString),
        ]
        return components?.url
    }

    private func applyPendingInviteContext() {
        guard let invite = appState.takePendingInviteContext() else { return }
        if let inviteCode = invite.inviteCode {
            redeemCode = inviteCode
        }
    }

    private static let rfc3339: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
}

private struct LocationEditorView: View {
    @Environment(\.dismiss) private var dismiss

    let location: Location?
    let onSave: (String, String) async -> Void

    @State private var name: String
    @State private var kind: String

    init(location: Location?, onSave: @escaping (String, String) async -> Void) {
        self.location = location
        self.onSave = onSave
        _name = State(initialValue: location?.name ?? "")
        _kind = State(initialValue: location?.kind ?? "pantry")
    }

    var body: some View {
        Form {
            TextField("Name", text: $name)
            Picker("Kind", selection: $kind) {
                Text("Pantry").tag("pantry")
                Text("Fridge").tag("fridge")
                Text("Freezer").tag("freezer")
            }
        }
        .navigationTitle(location == nil ? "New Location" : "Edit Location")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    Task {
                        await onSave(
                            name.trimmingCharacters(in: .whitespacesAndNewlines),
                            kind
                        )
                        dismiss()
                    }
                }
                .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }
}
