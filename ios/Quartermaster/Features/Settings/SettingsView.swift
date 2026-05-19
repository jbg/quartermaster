import CoreImage.CIFilterBuiltins
import SwiftUI
import UIKit

struct SettingsView: View {
  @Environment(AppState.self) private var appState

  @State private var household: HouseholdDetail?
  @State private var members: [Member] = []
  @State private var invites: [Invite] = []
  @State private var locations: [Location] = []
  @State private var storageVessels: [StorageVessel] = []

  @State private var householdNameDraft: String = ""
  @State private var householdTimezoneDraft: String = TimeZone.autoupdatingCurrent.identifier
  @State private var householdMeasurementSystemDraft: MeasurementSystem = .metric
  @State private var newInviteRole: MembershipRole = .readWrite
  @State private var recoveryEmailDraft: String = ""
  @State private var recoveryCodeDraft: String = ""
  @State private var offCredentialStatus: OpenFoodFactsCredentialStatusResponse?
  @State private var offUsernameDraft: String = ""
  @State private var offPasswordDraft: String = ""
  @State private var isSavingOFFCredentials = false
  @State private var passkeyLabelDraft: String = ""
  @State private var handoffTargetLabelDraft: String = UIDevice.current.name

  @State private var showLocationEditor = false
  @State private var editingLocation: Location?
  @State private var showStorageVesselEditor = false
  @State private var editingStorageVessel: StorageVessel?

  @State private var isLoading = true
  @State private var isSavingHousehold = false
  @State private var isCreatingInvite = false
  @State private var isSavingRecoveryEmail = false
  @State private var isExportingBackup = false
  @State private var isDeletingHousehold = false
  @State private var deletionConfirmationName = ""
  @State private var exportedBackupURL: URL?
  @State private var householdDataMessage: String?
  @State private var errorMessage: String?
  @State private var showRenameConfirmation = false
  @State private var invitePendingRevocation: Invite?
  @State private var memberPendingRemoval: Member?
  @State private var locationPendingDeletion: Location?
  @State private var storageVesselPendingDeletion: StorageVessel?
  @State private var householdEntry = HouseholdEntryController()
  private let passkeyAuthenticator = PasskeyAuthenticator()

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
          if isAdmin {
            NavigationLink {
              householdDataView
            } label: {
              settingsLinkLabel(
                "Backup & deletion",
                systemImage: "externaldrive.badge.timemachine",
                detail: householdDataMessage
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
            storageVesselsView
          } label: {
            settingsLinkLabel(
              "Storage Vessels",
              systemImage: "scalemass",
              detail: storageVessels.isEmpty ? "No vessels" : "\(storageVessels.count)"
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
              "Profile", systemImage: "person.crop.circle", detail: me.user.displayName)
          }
          NavigationLink {
            recoveryEmailView(for: me)
          } label: {
            settingsLinkLabel(
              "Email",
              systemImage: "envelope",
              detail: me.user.email
            )
          }
          NavigationLink {
            passkeysView
          } label: {
            settingsLinkLabel(
              "Passkeys",
              systemImage: "key",
              detail: appState.passkeys.isEmpty ? "Not configured" : "\(appState.passkeys.count)"
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
        NavigationLink {
          handoffView
        } label: {
          settingsLinkLabel("Pair signed-in device", systemImage: "qrcode", detail: nil)
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
    .sheet(isPresented: $showStorageVesselEditor) {
      NavigationStack {
        StorageVesselEditorView(vessel: editingStorageVessel) { name, tareWeight, tareUnit in
          await saveStorageVessel(name: name, tareWeight: tareWeight, tareUnit: tareUnit)
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
        Text("\(memberPendingRemoval.user.displayName) will lose access to this household.")
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
    .confirmationDialog(
      "Delete storage vessel?",
      isPresented: Binding(
        get: { storageVesselPendingDeletion != nil },
        set: { if !$0 { storageVesselPendingDeletion = nil } }
      ),
      titleVisibility: .visible
    ) {
      Button("Delete Storage Vessel", role: .destructive) {
        guard let storageVesselPendingDeletion else { return }
        Task { await deleteStorageVessel(storageVesselPendingDeletion) }
      }
      Button("Cancel", role: .cancel) {}
    } message: {
      if let storageVesselPendingDeletion {
        Text("\(storageVesselPendingDeletion.name) will be removed from future batch selection.")
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
      || householdMeasurementSystemDraft != household.measurementSystem
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
    if householdMeasurementSystemDraft != household.measurementSystem {
      changes.append(
        "change measurement system from \(household.measurementSystem.displayName) to \(householdMeasurementSystemDraft.displayName)"
      )
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
          LabeledContent("Role", value: currentRole?.displayName ?? "Read-write")
          LabeledContent("Timezone", value: household.timezone)
          LabeledContent("Measurement system", value: household.measurementSystem.displayName)
          Text(household.measurementSystem.detail)
            .font(.footnote)
            .foregroundStyle(.secondary)
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
            Picker("Measurement system", selection: $householdMeasurementSystemDraft) {
              ForEach(MeasurementSystem.supportedOptions, id: \.rawValue) { measure in
                Text(measure.displayName).tag(measure)
              }
            }
            Text(householdMeasurementSystemDraft.detail)
              .font(.footnote)
              .foregroundStyle(.secondary)
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

  private var householdDataView: some View {
    Form {
      Section {
        Button {
          Task { await exportBackup() }
        } label: {
          if isExportingBackup {
            ProgressView()
          } else {
            Label("Export backup", systemImage: "square.and.arrow.up")
          }
        }
        .disabled(isExportingBackup)

        if let exportedBackupURL {
          ShareLink(item: exportedBackupURL) {
            Label("Share exported backup", systemImage: "square.and.arrow.up")
          }
        }

        HouseholdBackupImportButton(title: "Import backup")

        if let householdDataMessage {
          Text(householdDataMessage)
            .font(.footnote)
            .foregroundStyle(.secondary)
        }
      } header: {
        Text("Backup")
      } footer: {
        Text("Backups are JSON files that restore into a new household copy.")
      }

      if let household {
        Section {
          Text("Export a backup first if you want to keep this data.")
            .font(.footnote)
            .foregroundStyle(.secondary)
          TextField("Type \(household.name)", text: $deletionConfirmationName)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
          Button(role: .destructive) {
            Task { await deleteCurrentHousehold() }
          } label: {
            if isDeletingHousehold {
              ProgressView()
            } else {
              Text("Delete household")
            }
          }
          .disabled(isDeletingHousehold || deletionConfirmationName != household.name)
        } header: {
          Text("Delete Household")
        } footer: {
          Text("Deletion is queued on the server and removes this household from your account.")
        }
      }
    }
    .navigationTitle("Household Data")
  }

  private var invitesView: some View {
    Form {
      Section("New Invite") {
        Picker("Role", selection: $newInviteRole) {
          Text("Read-only").tag(MembershipRole.readOnly)
          Text("Read-write").tag(MembershipRole.readWrite)
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

  private var storageVesselsView: some View {
    Form {
      Section("Storage Vessels") {
        if storageVessels.isEmpty {
          Text("No storage vessels yet.")
            .foregroundStyle(.secondary)
        } else {
          ForEach(storageVessels) { vessel in
            storageVesselRow(vessel)
          }
          .onMove { source, destination in
            Task { await moveStorageVessels(from: source, to: destination) }
          }
        }

        if isAdmin {
          Button {
            editingStorageVessel = nil
            showStorageVesselEditor = true
          } label: {
            Label("Add storage vessel", systemImage: "plus")
          }
        }
      }
    }
    .navigationTitle("Storage Vessels")
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
        LabeledContent("Display name", value: me.user.displayName)
        LabeledContent("Email", value: me.user.email)
      }
    }
    .navigationTitle("Profile")
  }

  private func recoveryEmailView(for me: Me) -> some View {
    Form {
      Section("Email") {
        recoveryEmailContent(for: me)
      }
    }
    .navigationTitle("Email")
  }

  private var passkeysView: some View {
    Form {
      Section("Passkeys") {
        if appState.passkeys.isEmpty {
          Text("No passkeys yet.")
            .foregroundStyle(.secondary)
        } else {
          ForEach(appState.passkeys) { passkey in
            VStack(alignment: .leading, spacing: 4) {
              Text(passkey.label ?? "Passkey")
              Text(passkey.lastUsedAt.map { "Last used \($0)" } ?? "Created \(passkey.createdAt)")
                .font(.footnote)
                .foregroundStyle(.secondary)
            }
            .swipeActions {
              Button(role: .destructive) {
                Task { await appState.deletePasskey(id: passkey.id) }
              } label: {
                Label("Delete", systemImage: "trash")
              }
            }
          }
        }
      }

      Section("Add") {
        TextField("Label", text: $passkeyLabelDraft)
        Button {
          Task {
            let label = passkeyLabelDraft.trimmingCharacters(in: .whitespacesAndNewlines)
            let start = try await appState.api.startPasskeyRegistration(
              label: label.isEmpty ? nil : label)
            let credentialJSON = try await passkeyAuthenticator.register(start: start)
            await appState.finishPasskeyRegistration(
              ceremonyID: start.ceremonyID,
              credentialJSON: credentialJSON,
              label: label.isEmpty ? nil : label
            )
            passkeyLabelDraft = ""
          }
        } label: {
          Label("Add passkey", systemImage: "key")
        }
      }
    }
    .navigationTitle("Passkeys")
  }

  private var handoffView: some View {
    Form {
      Section("Target") {
        TextField("Device label", text: $handoffTargetLabelDraft)
        Button {
          Task {
            await appState.createAuthHandoff(
              targetDeviceLabel: handoffTargetLabelDraft.trimmingCharacters(
                in: .whitespacesAndNewlines)
            )
          }
        } label: {
          Label("Create handoff code", systemImage: "qrcode")
        }
      }

      if let handoff = appState.authHandoff {
        Section("Code") {
          QRCodeImage(text: handoff.handoffURL)
            .frame(maxWidth: .infinity)
          Text(handoff.handoffURL)
            .font(.footnote.monospaced())
            .textSelection(.enabled)
          LabeledContent("Expires", value: handoff.expiresAt)
          Button(role: .destructive) {
            Task { await appState.cancelAuthHandoff() }
          } label: {
            Label("Cancel handoff", systemImage: "xmark.circle")
          }
        }
      }
    }
    .navigationTitle("Pair signed-in device")
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
    if let pending = me.user.pendingEmail {
      LabeledContent("Pending", value: pending)
      if let expiresAt = me.user.pendingEmailVerificationExpiresAt {
        LabeledContent("Code expires", value: expiresAt)
      }
    } else {
      LabeledContent("Email", value: me.user.email)
    }

    TextField("Email", text: $recoveryEmailDraft)
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

    Text("Verification codes are delivered to the account email.")
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
        Text(member.user.displayName)
        Text(member.user.email)
          .font(.footnote)
          .foregroundStyle(.secondary)
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

  private func storageVesselRow(_ vessel: StorageVessel) -> some View {
    HStack {
      VStack(alignment: .leading, spacing: 2) {
        Text(vessel.name)
        Text(vessel.displayTare)
          .font(.footnote)
          .foregroundStyle(.secondary)
      }
      Spacer()
      if isAdmin {
        Image(systemName: "line.3.horizontal")
          .foregroundStyle(.tertiary)
          .accessibilityHidden(true)
        Button {
          editingStorageVessel = vessel
          showStorageVesselEditor = true
        } label: {
          Image(systemName: "pencil")
        }
        Button(role: .destructive) {
          storageVesselPendingDeletion = vessel
        } label: {
          Image(systemName: "trash")
        }
      }
    }
    .buttonStyle(.borderless)
    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
      if isAdmin {
        Button(role: .destructive) {
          storageVesselPendingDeletion = vessel
        } label: {
          Label("Delete", systemImage: "trash")
        }
        Button {
          editingStorageVessel = vessel
          showStorageVesselEditor = true
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
      householdTimezoneDraft = TimeZone.autoupdatingCurrent.identifier
      householdMeasurementSystemDraft = .metric
      members = []
      invites = []
      locations = []
      storageVessels = []
      isLoading = false
      return
    }
    isLoading = true
    defer { isLoading = false }
    do {
      async let householdReq = appState.api.currentHousehold()
      async let membersReq = appState.api.householdMembers()
      async let locationsReq = appState.api.locations()
      async let storageVesselsReq = appState.api.storageVessels()
      async let offReq = appState.api.openFoodFactsCredentialStatus()
      async let passkeysReq = appState.api.listPasskeys()
      let (household, members, locations, storageVessels, offCredentialStatus, passkeys) = try await
        (
          householdReq, membersReq, locationsReq, storageVesselsReq, offReq, passkeysReq
        )
      self.household = household
      self.householdNameDraft = household.name
      self.householdTimezoneDraft = household.timezone
      self.householdMeasurementSystemDraft = household.measurementSystem
      self.members = members
      self.locations = locations.sorted { $0.sortOrder < $1.sortOrder }
      self.storageVessels = storageVessels.sorted { $0.sortOrder < $1.sortOrder }
      self.offCredentialStatus = offCredentialStatus
      appState.passkeys = passkeys
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
          householdTimezoneDraft = TimeZone.autoupdatingCurrent.identifier
          householdMeasurementSystemDraft = .metric
          members = []
          invites = []
          locations = []
          storageVessels = []
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
        timezone: householdTimezoneDraft,
        measurementSystem: householdMeasurementSystemDraft
      )
      household = updated
      await appState.refreshMe()
      await appState.refreshUnits()
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func exportBackup() async {
    guard let household else { return }
    isExportingBackup = true
    defer { isExportingBackup = false }
    do {
      let document = try await appState.api.exportCurrentHousehold()
      let data = try JSONEncoder().encode(document)
      let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(backupFileName(for: household.name))
      try data.write(to: url, options: .atomic)
      exportedBackupURL = url
      householdDataMessage = "Backup exported."
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func deleteCurrentHousehold() async {
    guard let household, deletionConfirmationName == household.name else { return }
    isDeletingHousehold = true
    defer { isDeletingHousehold = false }
    do {
      _ = try await appState.requestCurrentHouseholdDeletion(
        confirmationName: deletionConfirmationName)
      self.household = nil
      deletionConfirmationName = ""
      householdDataMessage = "Household deletion queued."
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

  private func saveStorageVessel(name: String, tareWeight: String, tareUnit: String) async {
    do {
      if let editingStorageVessel {
        _ = try await appState.api.updateStorageVessel(
          id: editingStorageVessel.id,
          name: name,
          tareWeight: tareWeight,
          tareUnit: tareUnit,
          sortOrder: Int(editingStorageVessel.sortOrder)
        )
      } else {
        _ = try await appState.api.createStorageVessel(
          name: name,
          tareWeight: tareWeight,
          tareUnit: tareUnit
        )
      }
      showStorageVesselEditor = false
      self.editingStorageVessel = nil
      storageVessels = try await appState.api.storageVessels().sorted {
        $0.sortOrder < $1.sortOrder
      }
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

  private func moveStorageVessels(from source: IndexSet, to destination: Int) async {
    var reordered = storageVessels
    reordered.move(fromOffsets: source, toOffset: destination)
    storageVessels = reordered
    do {
      for (idx, vessel) in reordered.enumerated() {
        _ = try await appState.api.updateStorageVessel(
          id: vessel.id,
          name: vessel.name,
          tareWeight: vessel.tareWeight,
          tareUnit: vessel.tareUnit,
          sortOrder: idx
        )
      }
      storageVessels = try await appState.api.storageVessels().sorted {
        $0.sortOrder < $1.sortOrder
      }
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
      await reloadStorageVesselsAfterFailedMove()
    } catch {
      errorMessage = error.localizedDescription
      await reloadStorageVesselsAfterFailedMove()
    }
  }

  private func reloadLocationsAfterFailedMove() async {
    do {
      locations = try await appState.api.locations().sorted { $0.sortOrder < $1.sortOrder }
    } catch {
      // Keep the original reorder error visible.
    }
  }

  private func reloadStorageVesselsAfterFailedMove() async {
    do {
      storageVessels = try await appState.api.storageVessels().sorted {
        $0.sortOrder < $1.sortOrder
      }
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

  private func deleteStorageVessel(_ vessel: StorageVessel) async {
    defer { storageVesselPendingDeletion = nil }
    do {
      try await appState.api.deleteStorageVessel(id: vessel.id)
      storageVessels.removeAll { $0.id == vessel.id }
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

  private func backupFileName(for householdName: String) -> String {
    let safeName =
      householdName
      .lowercased()
      .components(separatedBy: CharacterSet.alphanumerics.inverted)
      .filter { !$0.isEmpty }
      .joined(separator: "-")
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withFullDate]
    let name = safeName.isEmpty ? "household" : safeName
    let date = formatter.string(from: Date())
    return "quartermaster-\(name)-\(date).json"
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

private struct StorageVesselEditorView: View {
  @Environment(\.dismiss) private var dismiss

  let vessel: StorageVessel?
  let onSave: (String, String, String) async -> Void

  @State private var name: String
  @State private var tareWeight: String
  @State private var tareUnit: String

  init(vessel: StorageVessel?, onSave: @escaping (String, String, String) async -> Void) {
    self.vessel = vessel
    self.onSave = onSave
    _name = State(initialValue: vessel?.name ?? "")
    _tareWeight = State(initialValue: vessel?.tareWeight ?? "")
    _tareUnit = State(initialValue: vessel?.tareUnit ?? "g")
  }

  var body: some View {
    Form {
      TextField("Name", text: $name)
      DecimalField(title: "Tare weight", text: $tareWeight)
      Picker("Tare unit", selection: $tareUnit) {
        Text("g").tag("g")
        Text("kg").tag("kg")
        Text("oz").tag("oz")
        Text("lb").tag("lb")
      }
    }
    .navigationTitle(vessel == nil ? "New Storage Vessel" : "Edit Storage Vessel")
    .toolbar {
      ToolbarItem(placement: .cancellationAction) {
        Button("Cancel") { dismiss() }
      }
      ToolbarItem(placement: .confirmationAction) {
        Button("Save") {
          Task {
            await onSave(
              name.trimmingCharacters(in: .whitespacesAndNewlines),
              tareWeight.trimmingCharacters(in: .whitespacesAndNewlines),
              tareUnit
            )
            dismiss()
          }
        }
        .disabled(!canSave)
      }
    }
  }

  private var canSave: Bool {
    let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedWeight = tareWeight.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedName.isEmpty, let weight = Decimal(string: trimmedWeight) else { return false }
    return weight >= 0
  }
}

private struct QRCodeImage: View {
  let text: String
  private let context = CIContext()
  private let filter = CIFilter.qrCodeGenerator()

  var body: some View {
    if let image {
      Image(uiImage: image)
        .interpolation(.none)
        .resizable()
        .scaledToFit()
        .frame(width: 220, height: 220)
        .accessibilityLabel("Authenticated handoff QR code")
    }
  }

  private var image: UIImage? {
    filter.message = Data(text.utf8)
    guard let output = filter.outputImage else { return nil }
    let scaled = output.transformed(by: CGAffineTransform(scaleX: 8, y: 8))
    guard let cgImage = context.createCGImage(scaled, from: scaled.extent) else { return nil }
    return UIImage(cgImage: cgImage)
  }
}
