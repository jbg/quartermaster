import SwiftUI
import UniformTypeIdentifiers

struct HouseholdSwitcherMenu: View {
  let me: Me
  let isSwitching: Bool
  let onSwitch: (String) -> Void

  var body: some View {
    Menu {
      ForEach(me.households) { membership in
        Button {
          onSwitch(membership.id)
        } label: {
          HStack {
            VStack(alignment: .leading, spacing: 2) {
              Text(membership.name)
              Text(membership.role.displayName)
            }
            if me.currentHouseholdSummary?.id == membership.id {
              Image(systemName: "checkmark")
            }
          }
        }
        .disabled(isSwitching || me.currentHouseholdSummary?.id == membership.id)
      }
      Divider()
      HouseholdBackupImportButton()
    } label: {
      Label(me.currentHouseholdSummary?.name ?? "Households", systemImage: "person.2")
        .labelStyle(.titleAndIcon)
    }
    .disabled(isSwitching)
  }
}

struct HouseholdBackupImportButton: View {
  @Environment(AppState.self) private var appState

  let title: String
  let systemImage: String

  @State private var isImporting = false
  @State private var errorMessage: String?

  init(title: String = "Import backup", systemImage: String = "square.and.arrow.down") {
    self.title = title
    self.systemImage = systemImage
  }

  var body: some View {
    Button {
      isImporting = true
    } label: {
      Label(title, systemImage: systemImage)
    }
    .fileImporter(
      isPresented: $isImporting,
      allowedContentTypes: [.json],
      allowsMultipleSelection: false
    ) { result in
      Task { await importBackup(result) }
    }
    .alert(
      "Backup couldn't be imported",
      isPresented: Binding(
        get: { errorMessage != nil },
        set: { if !$0 { errorMessage = nil } }
      )
    ) {
      Button("OK", role: .cancel) {}
    } message: {
      Text(errorMessage ?? "")
    }
  }

  private func importBackup(_ result: Result<[URL], Error>) async {
    do {
      guard let url = try result.get().first else { return }
      let scoped = url.startAccessingSecurityScopedResource()
      defer {
        if scoped {
          url.stopAccessingSecurityScopedResource()
        }
      }
      let data = try Data(contentsOf: url)
      let document = try JSONDecoder().decode(HouseholdExportDocument.self, from: data)
      _ = try await appState.importHouseholdBackup(document)
    } catch let err as APIError {
      errorMessage = err.userFacingMessage
    } catch {
      errorMessage = error.localizedDescription
    }
  }
}
