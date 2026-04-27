import SwiftUI

struct OnboardingView: View {
  enum Mode: String, CaseIterable, Identifiable {
    case signIn = "Sign in"
    case createFirst = "Get started"
    var id: Self { self }
  }

  @Environment(AppState.self) private var appState
  @State private var mode: Mode = .createFirst
  @State private var username: String = ""
  @State private var password: String = ""
  @State private var email: String = ""
  @State private var inviteCode: String = ""
  @State private var serverURL: String = ""
  @State private var isSubmitting = false

  var body: some View {
    NavigationStack {
      Form {
        Section {
          Picker("", selection: $mode) {
            ForEach(Mode.allCases) { m in
              Text(m.rawValue).tag(m)
            }
          }
          .pickerStyle(.segmented)
          .listRowBackground(Color.clear)
          .labelsHidden()
        } header: {
          brandHeader
        }

        Section("Server") {
          TextField("Server URL", text: $serverURL, prompt: Text(appState.serverURL.absoluteString))
            .textContentType(.URL)
            .textInputAutocapitalization(.never)
            .keyboardType(.URL)
            .autocorrectionDisabled()
        }

        Section("Account") {
          TextField("Username", text: $username)
            .textContentType(.username)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
          SecureField("Password", text: $password)
            .textContentType(mode == .signIn ? .password : .newPassword)
          if mode == .createFirst {
            TextField("Email (optional)", text: $email)
              .textContentType(.emailAddress)
              .keyboardType(.emailAddress)
              .textInputAutocapitalization(.never)
              .autocorrectionDisabled()
            TextField("Invite code (optional)", text: $inviteCode)
              .textInputAutocapitalization(.characters)
              .autocorrectionDisabled()
          }
        }

        if let message = appState.lastError {
          Section {
            Text(message)
              .foregroundStyle(Color.quartermasterError)
          }
        }

        Section {
          Button {
            Task { await submit() }
          } label: {
            HStack {
              Spacer()
              if isSubmitting {
                ProgressView()
              } else {
                Text(mode == .signIn ? "Sign in" : "Create household")
                  .fontWeight(.semibold)
              }
              Spacer()
            }
          }
          .disabled(!canSubmit || isSubmitting)
        }

        if mode == .createFirst {
          Section {
            Text(
              "On open servers this creates a new household. On invite-only servers, enter an invite code from an admin."
            )
            .font(.footnote)
            .foregroundStyle(.secondary)
          }
        }
      }
      .navigationTitle("Quartermaster")
      .navigationBarTitleDisplayMode(.inline)
      .task { applyPendingInviteContext() }
      .onChange(of: appState.pendingInviteContext) { _, _ in
        applyPendingInviteContext()
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

  private var canSubmit: Bool {
    !username.trimmingCharacters(in: .whitespaces).isEmpty
      && password.count >= 8
  }

  private func submit() async {
    if let url = URL(string: serverURL), !serverURL.isEmpty {
      appState.updateServerURL(url)
    }
    isSubmitting = true
    defer { isSubmitting = false }

    switch mode {
    case .signIn:
      await appState.login(username: username, password: password)
    case .createFirst:
      let trimmedEmail = email.trimmingCharacters(in: .whitespaces)
      let trimmedInvite = inviteCode.trimmingCharacters(in: .whitespacesAndNewlines)
      await appState.register(
        username: username,
        password: password,
        email: trimmedEmail.isEmpty ? nil : trimmedEmail,
        inviteCode: trimmedInvite.isEmpty ? nil : trimmedInvite,
      )
    }
  }

  private func applyPendingInviteContext() {
    guard let invite = appState.takePendingInviteContext() else { return }
    mode = .createFirst
    if let serverURL = invite.serverURL {
      self.serverURL = serverURL.absoluteString
    }
    if let inviteCode = invite.inviteCode {
      self.inviteCode = inviteCode
    }
  }
}
