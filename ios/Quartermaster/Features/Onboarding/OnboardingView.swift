import SwiftUI
import VisionKit

struct OnboardingView: View {
  enum AuthMode: String, CaseIterable, Identifiable {
    case signIn = "Sign in"
    case createHousehold = "Create household"
    var id: Self { self }
  }

  @Environment(AppState.self) private var appState
  @State private var status: OnboardingStatus?
  @State private var authMode: AuthMode = .signIn
  @State private var username: String = ""
  @State private var password: String = ""
  @State private var householdName: String = ""
  @State private var timezone: String = TimeZone.autoupdatingCurrent.identifier
  @State private var serverURL: String = ""
  @State private var showAdvanced = false
  @State private var showScanner = false
  @State private var isConnecting = false
  @State private var isSubmitting = false
  @State private var localError: String?

  var body: some View {
    NavigationStack {
      Form {
        Section {
          brandHeader
        }
        .listRowBackground(Color.clear)

        if let status {
          connectedServerSection
          if let inviteCode = appState.pendingInviteContext?.inviteCode, !inviteCode.isEmpty {
            joinInviteSection(inviteCode: inviteCode)
          } else if status.serverState == .needsInitialSetup {
            createHouseholdSection(title: "Set up this server", buttonTitle: "Set up server")
          } else {
            authChoiceSection(status: status)
          }
        } else {
          connectSection
        }

        if let message = localError ?? appState.lastError {
          Section {
            Text(message)
              .foregroundStyle(Color.quartermasterError)
          }
        }
      }
      .navigationTitle("Quartermaster")
      .navigationBarTitleDisplayMode(.inline)
      .task {
        serverURL = appState.serverURL.absoluteString
        applyPendingInviteContext()
      }
      .onChange(of: appState.pendingInviteContext) { _, _ in
        applyPendingInviteContext()
      }
      .sheet(isPresented: $showScanner) {
        SetupScannerView { value in
          showScanner = false
          handleScannedSetup(value)
        }
      }
    }
  }

  private var brandHeader: some View {
    HStack(spacing: 12) {
      Image(systemName: "shippingbox.circle.fill")
        .font(.system(size: 36))
        .foregroundStyle(QuartermasterBrand.green800)
      VStack(alignment: .leading, spacing: 2) {
        Text("Quartermaster")
          .font(.title2.weight(.semibold))
          .foregroundStyle(.primary)
        Text("Kitchen inventory, kept in order.")
          .font(.footnote)
          .foregroundStyle(.secondary)
      }
    }
    .padding(.vertical, 8)
    .textCase(nil)
  }

  private var connectSection: some View {
    Section("Connect") {
      Button {
        showScanner = true
      } label: {
        Label("Scan setup code", systemImage: "qrcode.viewfinder")
          .fontWeight(.semibold)
      }
      .disabled(isConnecting)

      DisclosureGroup("Advanced", isExpanded: $showAdvanced) {
        TextField("Server URL", text: $serverURL, prompt: Text(appState.serverURL.absoluteString))
          .textContentType(.URL)
          .textInputAutocapitalization(.never)
          .keyboardType(.URL)
          .autocorrectionDisabled()
        Button {
          Task { await connectManualServer() }
        } label: {
          if isConnecting {
            ProgressView()
          } else {
            Text("Connect")
          }
        }
        .disabled(isConnecting)
      }
    }
  }

  private var connectedServerSection: some View {
    Section("Server") {
      LabeledContent("Connected", value: appState.serverURL.absoluteString)
      Button("Change server") {
        status = nil
        localError = nil
        appState.pendingInviteContext = nil
      }
    }
  }

  @ViewBuilder
  private func authChoiceSection(status: OnboardingStatus) -> some View {
    Section {
      Picker("", selection: $authMode) {
        ForEach(availableAuthModes(status: status)) { mode in
          Text(mode.rawValue).tag(mode)
        }
      }
      .pickerStyle(.segmented)
      .labelsHidden()
    } header: {
      Text("Account")
    }

    if authMode == .createHousehold {
      createHouseholdSection(title: "Create household", buttonTitle: "Create household")
    } else {
      signInSection
    }
  }

  private var signInSection: some View {
    Section("Sign in") {
      accountFields(passwordContentType: .password)
      Button {
        Task { await submitSignIn() }
      } label: {
        submitLabel("Sign in")
      }
      .disabled(!canSubmitAccount || isSubmitting)
    }
  }

  private func createHouseholdSection(title: String, buttonTitle: String) -> some View {
    Section(title) {
      accountFields(passwordContentType: .newPassword)
      TextField("Household name", text: $householdName)
        .textContentType(.organizationName)
      TextField("Timezone", text: $timezone)
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
      Button {
        Task { await submitCreateHousehold() }
      } label: {
        submitLabel(buttonTitle)
      }
      .disabled(!canSubmitHousehold || isSubmitting)
    }
  }

  private func joinInviteSection(inviteCode: String) -> some View {
    Section("Join household") {
      LabeledContent("Invite", value: inviteCode)
      accountFields(passwordContentType: .newPassword)
      Button {
        Task { await submitJoinInvite(inviteCode: inviteCode) }
      } label: {
        submitLabel("Join household")
      }
      .disabled(!canSubmitAccount || isSubmitting)
    }
  }

  private func accountFields(passwordContentType: UITextContentType) -> some View {
    Group {
      TextField("Username", text: $username)
        .textContentType(.username)
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
      SecureField("Password", text: $password)
        .textContentType(passwordContentType)
    }
  }

  private func submitLabel(_ title: String) -> some View {
    HStack {
      Spacer()
      if isSubmitting {
        ProgressView()
      } else {
        Text(title).fontWeight(.semibold)
      }
      Spacer()
    }
  }

  private var canSubmitAccount: Bool {
    !username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && password.count >= 8
  }

  private var canSubmitHousehold: Bool {
    canSubmitAccount
      && !householdName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      && !timezone.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  private func availableAuthModes(status: OnboardingStatus) -> [AuthMode] {
    status.householdSignup == .enabled ? [.signIn, .createHousehold] : [.signIn]
  }

  private func connectManualServer() async {
    let raw = serverURL.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let url = URL(string: raw), ["http", "https"].contains(url.scheme?.lowercased() ?? "")
    else {
      localError = "Enter a valid server URL."
      return
    }
    appState.updateServerURL(url)
    await loadStatus()
  }

  private func handleScannedSetup(_ value: String) {
    guard let url = URL(string: value.trimmingCharacters(in: .whitespacesAndNewlines)) else {
      localError = "That setup code is not a Quartermaster link."
      return
    }
    if ["http", "https"].contains(url.scheme?.lowercased() ?? ""), url.path != "/join" {
      appState.updateServerURL(url)
    } else {
      appState.handleIncomingURL(url)
    }
    serverURL = appState.serverURL.absoluteString
    Task { await loadStatus() }
  }

  private func loadStatus() async {
    isConnecting = true
    localError = nil
    defer { isConnecting = false }
    do {
      status = try await appState.onboardingStatus()
      if status?.householdSignup == .enabled {
        authMode = status?.serverState == .needsInitialSetup ? .createHousehold : authMode
      } else {
        authMode = .signIn
      }
    } catch {
      localError = (error as? APIError)?.userFacingMessage ?? error.localizedDescription
    }
  }

  private func submitSignIn() async {
    isSubmitting = true
    defer { isSubmitting = false }
    await appState.login(username: username.trimmed, password: password)
  }

  private func submitCreateHousehold() async {
    isSubmitting = true
    defer { isSubmitting = false }
    await appState.createOnboardingHousehold(
      username: username.trimmed,
      password: password,
      householdName: householdName.trimmed,
      timezone: timezone.trimmed
    )
  }

  private func submitJoinInvite(inviteCode: String) async {
    isSubmitting = true
    defer { isSubmitting = false }
    await appState.joinOnboardingInvite(
      username: username.trimmed,
      password: password,
      inviteCode: inviteCode
    )
  }

  private func applyPendingInviteContext() {
    guard let invite = appState.pendingInviteContext else { return }
    if let serverURL = invite.serverURL {
      self.serverURL = serverURL.absoluteString
    }
    if status == nil {
      Task { await loadStatus() }
    }
  }
}

private struct SetupScannerView: View {
  @Environment(\.dismiss) private var dismiss
  var onCode: (String) -> Void

  var body: some View {
    NavigationStack {
      ZStack {
        if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
          ScannerView(onBarcode: onCode)
            .ignoresSafeArea(edges: [.bottom, .horizontal])
        } else {
          ContentUnavailableView {
            Label("Camera scanning unavailable", systemImage: "camera.slash")
          } description: {
            Text("Use Advanced to enter the server URL manually.")
          }
          .padding()
        }
      }
      .navigationTitle("Scan setup code")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .topBarTrailing) {
          Button("Cancel") { dismiss() }
        }
      }
    }
  }
}

extension String {
  fileprivate var trimmed: String {
    trimmingCharacters(in: .whitespacesAndNewlines)
  }
}
