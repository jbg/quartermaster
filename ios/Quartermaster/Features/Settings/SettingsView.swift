import SwiftUI

struct SettingsView: View {
  @Environment(AppState.self) private var appState

  @State private var household: HouseholdDetail?
  @State private var members: [Member] = []
  @State private var invites: [Invite] = []
  @State private var locations: [Location] = []

  @State private var householdNameDraft: String = ""
  @State private var householdTimezoneDraft: String = TimeZone.autoupdatingCurrent.identifier
  @State private var newInviteRole: MembershipRole = .member
  @State private var recoveryEmailDraft: String = ""
  @State private var recoveryCodeDraft: String = ""
  @State private var offCredentialStatus: OpenFoodFactsCredentialStatusResponse?
  @State private var offUsernameDraft: String = ""
  @State private var offPasswordDraft: String = ""
  @State private var isSavingOFFCredentials = false

  @State private var showLocationEditor = false
  @State private var editingLocation: Location?

  @State private var isLoading = true
  @State private var isSavingHousehold = false
  @State private var isCreatingInvite = false
  @State private var isSavingRecoveryEmail = false
  @State private var errorMessage: String?
  @State private var showRenameConfirmation = false
  @State private var invitePendingRevocation: Invite?
  @State private var memberPendingRemoval: Member?
  @State private var locationPendingDeletion: Location?
  @State private var householdEntry = HouseholdEntryController()

  var body: some View {
    Form {
      if let me {
        HouseholdEntrySections(
          controller: householdEntry,
          me: me,
          switchSectionTitle: "Switch Household",
          redeemSectionTitle: "Join another household",
          redeemActionTitle: "Redeem invite",
          showsCreateHousehold: false,
        ) {
          await load(retryOnForbidden: false)
        }
      }

      if household != nil {
        Section("Household") {
          NavigationLink {
            householdDetailsView
          } label: {
            settingsLinkLabel(
              "Details",
              systemImage: "house",
              detail: household?.name
            )
          }
          if isAdmin {
            NavigationLink {
              invitesView
            } label: {
              settingsLinkLabel(
                "Invites",
                systemImage: "person.2.badge.plus",
                detail: invites.isEmpty ? "No active invites" : "\(invites.count) active"
              )
            }
          }
          NavigationLink {
            membersView
          } label: {
            settingsLinkLabel(
              "Members",
              systemImage: "person.2",
              detail: members.isEmpty ? nil : "\(members.count)"
            )
          }
        }

        Section("Inventory") {
          NavigationLink {
            locationsView
          } label: {
            settingsLinkLabel(
              "Locations",
              systemImage: "shippingbox",
              detail: locations.isEmpty ? "No locations" : "\(locations.count)"
            )
          }
          NavigationLink {
            StockHistoryView(scope: .household)
          } label: {
            Label("Stock history", systemImage: "clock.arrow.circlepath")
          }
        }
      }

      if let me {
        Section("Account") {
          NavigationLink {
            accountView(for: me)
          } label: {
            settingsLinkLabel(
              "Profile", systemImage: "person.crop.circle", detail: me.user.username)
          }
          NavigationLink {
            recoveryEmailView(for: me)
          } label: {
            settingsLinkLabel(
              "Recovery Email",
              systemImage: "envelope",
              detail: me.user.email ?? me.user.pendingEmail ?? "Not configured"
            )
          }
          NavigationLink {
            openFoodFactsCredentialsView
          } label: {
            settingsLinkLabel(
              "Open Food Facts",
              systemImage: "square.and.pencil",
              detail: offCredentialStatus?.username ?? "Not configured"
            )
          }
        }
      }

      Section("Server") {
        NavigationLink {
          serverView
        } label: {
          settingsLinkLabel("Server", systemImage: "server.rack", detail: appState.serverURL.host)
        }
      }

      Section {
        Button(role: .destructive) {
          Task { await appState.logout() }
        } label: {
          Text("Sign out")
        }
      }

      Section {
        NavigationLink {
          aboutView
        } label: {
          Label("About", systemImage: "info.circle")
        }
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
      recoveryEmailDraft = me?.user.pendingEmail ?? me?.user.email ?? recoveryEmailDraft
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
    .alert(
      "Couldn't complete that action",
      isPresented: Binding(
        get: { errorMessage != nil },
        set: { if !$0 { errorMessage = nil } }
      )
    ) {
      Button("OK", role: .cancel) {}
    } message: {
      Text(errorMessage ?? "")
    }
    .confirmationDialog(
      "Save household changes?",
      isPresented: $showRenameConfirmation,
      titleVisibility: .visible
    ) {
      Button("Save") {
        Task { await saveHousehold() }
      }
      Button("Cancel", role: .cancel) {}
    } message: {
      Text(householdChangeConfirmationMessage)
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

  private var me: Me? { appState.me }

  private var currentRole: MembershipRole? {
    guard let me else { return nil }
    if let currentHouseholdID = me.currentHouseholdSummary?.id {
      return me.households.first(where: { $0.id == currentHouseholdID })?.role
    }
    return members.first(where: { $0.user.id == me.user.id })?.role
  }

  private var isAdmin: Bool {
    currentRole == .admin
  }

  private var trimmedHouseholdNameDraft: String {
    householdNameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var hasHouseholdDraftChanges: Bool {
    guard let household else { return false }
    return trimmedHouseholdNameDraft != household.name
      || householdTimezoneDraft != household.timezone
  }

  private var canSaveHousehold: Bool {
    !trimmedHouseholdNameDraft.isEmpty && hasHouseholdDraftChanges
  }

  private var householdChangeConfirmationMessage: String {
    guard let household else {
      return "Save household changes?"
    }

    var changes: [String] = []
    if trimmedHouseholdNameDraft != household.name {
      changes.append("rename \(household.name) to \(trimmedHouseholdNameDraft)")
    }
    if householdTimezoneDraft != household.timezone {
      changes.append("change timezone from \(household.timezone) to \(householdTimezoneDraft)")
    }

    let summary = changes.isEmpty ? "save these settings" : changes.joined(separator: " and ")
    return """
      This will \(summary).

      Existing stored dates stay unchanged; timezone edits only correct how household-local dates are interpreted.
      """
  }

  private func settingsLinkLabel(_ title: String, systemImage: String, detail: String? = nil)
    -> some View
  {
    Label {
      HStack {
        Text(title)
        Spacer()
        if let detail {
          Text(detail)
            .foregroundStyle(.secondary)
        }
      }
    } icon: {
      Image(systemName: systemImage)
    }
  }

  private var householdDetailsView: some View {
    Form {
      if let household {
        Section("Current Household") {
          LabeledContent("Name", value: household.name)
          LabeledContent("Role", value: currentRole?.displayName ?? "Member")
          LabeledContent("Timezone", value: household.timezone)
          LabeledContent("Device timezone", value: appState.deviceTimeZone.identifier)
          if appState.timezonesDiffer {
            Text("Expiry dates and reminder schedules follow household time.")
              .font(.footnote)
              .foregroundStyle(.secondary)
          }
        }

        if isAdmin {
          Section("Edit Details") {
            TextField("Household name", text: $householdNameDraft)
            Picker("Timezone", selection: $householdTimezoneDraft) {
              ForEach(TimeZone.knownTimeZoneIdentifiers, id: \.self) { identifier in
                Text(identifier).tag(identifier)
              }
            }
            Button {
              showRenameConfirmation = true
            } label: {
              if isSavingHousehold {
                ProgressView()
              } else {
                Text("Save household")
              }
            }
            .disabled(!canSaveHousehold || isSavingHousehold)
          }
        }
      }
    }
    .navigationTitle("Household")
  }

  private var invitesView: some View {
    Form {
      Section("New Invite") {
        Picker("Role", selection: $newInviteRole) {
          Text("Member").tag(MembershipRole.member)
          Text("Admin").tag(MembershipRole.admin)
        }
        Button {
          Task { await createInvite() }
        } label: {
          if isCreatingInvite {
            ProgressView()
          } else {
            Text("Create single-use invite")
          }
        }
        .disabled(isCreatingInvite)
      }

      Section("Active Invites") {
        if invites.isEmpty {
          Text("No active invites.")
            .foregroundStyle(.secondary)
        } else {
          ForEach(invites) { invite in
            inviteRow(invite)
          }
        }
      }
    }
    .navigationTitle("Invites")
  }

  private var membersView: some View {
    Form {
      Section {
        if members.isEmpty {
          Text("No members found.")
            .foregroundStyle(.secondary)
        } else {
          ForEach(members) { member in
            memberRow(member)
          }
        }
      }
    }
    .navigationTitle("Members")
  }

  private var locationsView: some View {
    Form {
      Section("Locations") {
        if locations.isEmpty {
          Text("No locations yet.")
            .foregroundStyle(.secondary)
        } else {
          ForEach(locations) { location in
            locationRow(location)
          }
          .onMove { source, destination in
            Task { await moveLocations(from: source, to: destination) }
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
    }
    .navigationTitle("Locations")
    .toolbar {
      if isAdmin {
        ToolbarItem(placement: .topBarTrailing) {
          EditButton()
        }
      }
    }
  }

  private func accountView(for me: Me) -> some View {
    Form {
      Section("Account") {
        LabeledContent("Username", value: me.user.username)
        if let email = me.user.email {
          LabeledContent("Email", value: email)
        }
      }
    }
    .navigationTitle("Profile")
  }

  private func recoveryEmailView(for me: Me) -> some View {
    Form {
      Section("Recovery Email") {
        recoveryEmailContent(for: me)
      }
    }
    .navigationTitle("Recovery Email")
  }

  private var openFoodFactsCredentialsView: some View {
    Form {
      Section {
        TextField("Username", text: $offUsernameDraft)
          .textInputAutocapitalization(.never)
          .autocorrectionDisabled()
        SecureField("Password", text: $offPasswordDraft)
        if let username = offCredentialStatus?.username {
          LabeledContent("Saved account", value: username)
        }
      } header: {
        Text("Open Food Facts")
      } footer: {
        Text(
          "Credentials are encrypted on this Quartermaster server and used only when you contribute product corrections."
        )
      }

      Section {
        Button {
          Task { await saveOFFCredentials() }
        } label: {
          if isSavingOFFCredentials {
            ProgressView()
          } else {
            Text("Save credentials")
          }
        }
        .disabled(
          isSavingOFFCredentials
            || offUsernameDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || offPasswordDraft.isEmpty)

        if offCredentialStatus?.configured == true {
          Button(role: .destructive) {
            Task { await deleteOFFCredentials() }
          } label: {
            Text("Remove credentials")
          }
          .disabled(isSavingOFFCredentials)
        }
      }
    }
    .navigationTitle("Open Food Facts")
  }

  private var serverView: some View {
    Form {
      Section("Server") {
        LabeledContent("URL", value: appState.serverURL.absoluteString)
      }
    }
    .navigationTitle("Server")
  }

  private var aboutView: some View {
    Form {
      Section("Product data attribution") {
        Text(
          "Barcode lookups use [Open Food Facts](https://world.openfoodfacts.org), an open database available under the [Open Database Licence (ODbL)](https://opendatacommons.org/licenses/odbl/1-0/)."
        )
        .font(.footnote)
        .foregroundStyle(.secondary)
      }

      Section {
        Text(Self.versionDisplay)
          .font(.footnote)
          .foregroundStyle(.secondary)
      }
    }
    .navigationTitle("About")
  }

  @ViewBuilder
  private func recoveryEmailContent(for me: Me) -> some View {
    if let email = me.user.email {
      LabeledContent("Verified", value: email)
    } else if let pending = me.user.pendingEmail {
      LabeledContent("Pending", value: pending)
      if let expiresAt = me.user.pendingEmailVerificationExpiresAt {
        LabeledContent("Code expires", value: expiresAt)
      }
    } else {
      Text("No recovery email configured.")
        .foregroundStyle(.secondary)
    }

    TextField("Recovery email", text: $recoveryEmailDraft)
      .textContentType(.emailAddress)
      .textInputAutocapitalization(.never)
      .keyboardType(.emailAddress)
      .autocorrectionDisabled()
    Button {
      Task { await requestRecoveryEmail() }
    } label: {
      if isSavingRecoveryEmail {
        ProgressView()
      } else {
        Text("Send verification code")
      }
    }
    .disabled(
      isSavingRecoveryEmail
        || recoveryEmailDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

    if me.user.pendingEmail != nil {
      TextField("Verification code", text: $recoveryCodeDraft)
        .textInputAutocapitalization(.characters)
        .autocorrectionDisabled()
      Button {
        Task { await confirmRecoveryEmail() }
      } label: {
        if isSavingRecoveryEmail {
          ProgressView()
        } else {
          Text("Confirm email")
        }
      }
      .disabled(
        isSavingRecoveryEmail
          || recoveryCodeDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }

    if me.user.email != nil || me.user.pendingEmail != nil {
      Button("Remove recovery email", role: .destructive) {
        Task { await clearRecoveryEmail() }
      }
      .disabled(isSavingRecoveryEmail)
    }

    Text("Verification codes are delivered to the recovery email.")
      .font(.footnote)
      .foregroundStyle(.secondary)
  }

  private func inviteRow(_ invite: Invite) -> some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(invite.code)
          .font(.headline.monospaced())
        Spacer()
        ShareLink(
          item: inviteShareText(invite),
          preview: SharePreview(
            "Quartermaster Invite", image: Image(systemName: "person.2.badge.plus"))
        ) {
          Image(systemName: "square.and.arrow.up")
        }
        Button(role: .destructive) {
          invitePendingRevocation = invite
        } label: {
          Image(systemName: "trash")
        }
      }
      Text(
        "\(invite.roleGranted.displayName) • single-use • \(invite.useCount)/\(invite.maxUses) used • expires \(invite.expiresAt)"
      )
      .font(.footnote)
      .foregroundStyle(.secondary)
    }
    .buttonStyle(.borderless)
  }

  private func memberRow(_ member: Member) -> some View {
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
    .buttonStyle(.borderless)
  }

  private func locationRow(_ location: Location) -> some View {
    HStack {
      VStack(alignment: .leading, spacing: 2) {
        Text(location.name)
        Text(location.kind.capitalized)
          .font(.footnote)
          .foregroundStyle(.secondary)
      }
      Spacer()
      if isAdmin {
        Image(systemName: "line.3.horizontal")
          .foregroundStyle(.tertiary)
          .accessibilityHidden(true)
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
    .buttonStyle(.borderless)
    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
      if isAdmin {
        Button(role: .destructive) {
          locationPendingDeletion = location
        } label: {
          Label("Delete", systemImage: "trash")
        }
        Button {
          editingLocation = location
          showLocationEditor = true
        } label: {
          Label("Edit", systemImage: "pencil")
        }
        .tint(.accentColor)
      }
    }
  }

  private func load(retryOnForbidden: Bool = true) async {
    guard let me else { return }
    guard me.currentHouseholdSummary != nil else {
      household = nil
      householdNameDraft = ""
      members = []
      invites = []
      locations = []
      isLoading = false
      return
    }
    isLoading = true
    defer { isLoading = false }
    do {
      async let householdReq = appState.api.currentHousehold()
      async let membersReq = appState.api.householdMembers()
      async let locationsReq = appState.api.locations()
      async let offReq = appState.api.openFoodFactsCredentialStatus()
      let (household, members, locations, offCredentialStatus) = try await (
        householdReq, membersReq, locationsReq, offReq
      )
      self.household = household
      self.householdNameDraft = household.name
      self.householdTimezoneDraft = household.timezone
      self.members = members
      self.locations = locations.sorted { $0.sortOrder < $1.sortOrder }
      self.offCredentialStatus = offCredentialStatus
      self.offUsernameDraft = offCredentialStatus.username ?? ""
      self.offPasswordDraft = ""
      if members.first(where: { $0.user.id == me.user.id })?.role == .admin {
        invites = try await appState.api.householdInvites()
      } else {
        invites = []
      }
      await appState.refreshRemindersSilently()
    } catch let err as APIError {
      if case .server(status: 403, _) = err, retryOnForbidden {
        switch await appState.resolveHouseholdScopedForbidden() {
        case .retry:
          await load(retryOnForbidden: false)
          return
        case .fallbackToNoHousehold:
          household = nil
          householdNameDraft = ""
          members = []
          invites = []
          locations = []
          isLoading = false
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
  }

  private func saveOFFCredentials() async {
    isSavingOFFCredentials = true
    defer { isSavingOFFCredentials = false }
    do {
      let saved = try await appState.api.saveOpenFoodFactsCredentials(
        username: offUsernameDraft.trimmingCharacters(in: .whitespacesAndNewlines),
        password: offPasswordDraft
      )
      offCredentialStatus = saved
      offUsernameDraft = saved.username ?? ""
      offPasswordDraft = ""
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func deleteOFFCredentials() async {
    isSavingOFFCredentials = true
    defer { isSavingOFFCredentials = false }
    do {
      try await appState.api.deleteOpenFoodFactsCredentials()
      offCredentialStatus = .init(configured: false, username: nil)
      offUsernameDraft = ""
      offPasswordDraft = ""
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func saveHousehold() async {
    isSavingHousehold = true
    defer { isSavingHousehold = false }
    do {
      let updated = try await appState.api.updateCurrentHousehold(
        name: trimmedHouseholdNameDraft,
        timezone: householdTimezoneDraft
      )
      household = updated
      await appState.refreshMe()
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
      _ = try await appState.api.createInvite(maxUses: 1, role: newInviteRole)
      invites = try await appState.api.householdInvites()
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func requestRecoveryEmail() async {
    isSavingRecoveryEmail = true
    defer { isSavingRecoveryEmail = false }
    await appState.requestEmailVerification(email: recoveryEmailDraft)
    await load(retryOnForbidden: false)
    recoveryEmailDraft = me?.user.pendingEmail ?? me?.user.email ?? recoveryEmailDraft
  }

  private func confirmRecoveryEmail() async {
    isSavingRecoveryEmail = true
    defer { isSavingRecoveryEmail = false }
    await appState.confirmEmailVerification(code: recoveryCodeDraft)
    recoveryCodeDraft = ""
    await load(retryOnForbidden: false)
    recoveryEmailDraft = me?.user.email ?? recoveryEmailDraft
  }

  private func clearRecoveryEmail() async {
    isSavingRecoveryEmail = true
    defer { isSavingRecoveryEmail = false }
    await appState.clearRecoveryEmail()
    recoveryCodeDraft = ""
    recoveryEmailDraft = ""
    await load(retryOnForbidden: false)
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
      locations = try await appState.api.locations().sorted { $0.sortOrder < $1.sortOrder }
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func moveLocations(from source: IndexSet, to destination: Int) async {
    var reordered = locations
    reordered.move(fromOffsets: source, toOffset: destination)
    locations = reordered
    do {
      for (idx, location) in reordered.enumerated() {
        _ = try await appState.api.updateLocation(
          id: location.id,
          name: location.name,
          kind: location.kind,
          sortOrder: idx
        )
      }
      locations = try await appState.api.locations().sorted { $0.sortOrder < $1.sortOrder }
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
      await reloadLocationsAfterFailedMove()
    } catch {
      errorMessage = error.localizedDescription
      await reloadLocationsAfterFailedMove()
    }
  }

  private func reloadLocationsAfterFailedMove() async {
    do {
      locations = try await appState.api.locations().sorted { $0.sortOrder < $1.sortOrder }
    } catch {
      // Keep the original reorder error visible.
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
    let link = inviteJoinURL(invite)?.absoluteString ?? inviteBaseURL.absoluteString
    return """
      Join \(householdName) in Quartermaster.

      Open link: \(link)
      Server URL: \(appState.serverURL.absoluteString)
      Invite code: \(invite.code)

      On supported iPhone installs the link can open Quartermaster directly. If it doesn’t, open Quartermaster and choose “Redeem invite”.
      """
  }

  private func inviteJoinURL(_ invite: Invite) -> URL? {
    var components = URLComponents(url: inviteBaseURL, resolvingAgainstBaseURL: false)
    components?.path = "/join"
    components?.queryItems = [
      URLQueryItem(name: "invite", value: invite.code),
      URLQueryItem(name: "server", value: appState.serverURL.absoluteString),
    ]
    return components?.url
  }

  private var inviteBaseURL: URL {
    if let raw = me?.publicBaseUrl,
      let url = URL(string: raw)
    {
      return url
    }
    return appState.serverURL
  }

  private func applyPendingInviteContext() {
    guard let invite = appState.takePendingInviteContext() else { return }
    if let inviteCode = invite.inviteCode {
      householdEntry.redeemCode = inviteCode
    }
  }

  private static let versionDisplay: String = {
    let short = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String
    let version: String
    switch (short, build) {
    case (let short?, let build?) where !short.isEmpty && !build.isEmpty:
      version = "\(short) (\(build))"
    case (let short?, _) where !short.isEmpty:
      version = short
    case (_, let build?) where !build.isEmpty:
      version = build
    default:
      version = "Unknown version"
    }
    return "Quartermaster • v\(version)"
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
