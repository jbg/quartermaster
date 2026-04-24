import SwiftUI

/// TextField wrapper that validates its input as an `http(s)` URL. Surfaces
/// an inline footer-style error when the current value is non-empty-and-
/// invalid, and publishes an `isValid` binding the containing form can use
/// to disable Save/Create.
struct ValidatedURLField: View {
  let title: LocalizedStringKey
  @Binding var text: String
  @Binding var isValid: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      TextField(title, text: $text)
        .textInputAutocapitalization(.never)
        .keyboardType(.URL)
        .autocorrectionDisabled()
        .onChange(of: text) { _, _ in recompute() }
        .onAppear { recompute() }

      if !isValid {
        Text("Must start with http:// or https://")
          .font(.caption)
          .foregroundStyle(.red)
      }
    }
  }

  private func recompute() {
    isValid = Self.isAcceptable(text)
  }

  /// Empty strings are valid (the field is optional). Non-empty strings
  /// must parse as a URL with an `http` or `https` scheme.
  static func isAcceptable(_ raw: String) -> Bool {
    let trimmed = raw.trimmingCharacters(in: .whitespaces)
    if trimmed.isEmpty { return true }
    guard let url = URL(string: trimmed), let scheme = url.scheme?.lowercased() else {
      return false
    }
    return scheme == "http" || scheme == "https"
  }
}
