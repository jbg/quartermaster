import SwiftUI

struct HouseholdSwitcherMenu: View {
    let me: Me
    let isSwitching: Bool
    let onSwitch: (String) -> Void

    var body: some View {
        Menu {
            ForEach(me.households) { membership in
                Button {
                    onSwitch(membership.household.id)
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(membership.household.name)
                            Text(membership.role.displayName)
                        }
                        if me.householdId == membership.household.id {
                            Image(systemName: "checkmark")
                        }
                    }
                }
                .disabled(isSwitching || me.householdId == membership.household.id)
            }
        } label: {
            Label(me.householdName ?? "Households", systemImage: "person.2")
                .labelStyle(.titleAndIcon)
        }
        .disabled(isSwitching || me.households.isEmpty)
    }
}
