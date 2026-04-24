import SwiftUI

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
    } label: {
      Label(me.currentHouseholdSummary?.name ?? "Households", systemImage: "person.2")
        .labelStyle(.titleAndIcon)
    }
    .disabled(isSwitching || me.households.isEmpty)
  }
}
