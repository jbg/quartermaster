import SwiftUI

struct ReminderInboxView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        List {
            if appState.timezonesDiffer {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Household time applies")
                            .font(.subheadline.weight(.semibold))
                        Text(
                            "Reminders and expiry dates follow \(appState.householdTimeZoneID ?? "the household timezone"), while this device is currently in \(appState.deviceTimeZone.identifier)."
                        )
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 2)
                }
            }

            Section {
                if appState.reminders.isEmpty {
                    ContentUnavailableView(
                        "No reminders",
                        systemImage: "bell.slash",
                        description: Text("Due reminders will show up here.")
                    )
                    .frame(maxWidth: .infinity)
                } else {
                    ForEach(appState.reminders) { reminder in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(reminder.title)
                                .font(.headline)
                            Text(reminder.body)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                            if let expiresOn = reminder.expiresOn {
                                Text("Expires \(appState.displayDate(for: expiresOn) ?? expiresOn)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            if appState.timezonesDiffer, let local = reminder.fireAtDate {
                                Text("Household time \(reminder.householdFireLocalAt) · Here \(Self.localTime.string(from: local))")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            HStack {
                                Button("Open") {
                                    appState.pendingInventoryTarget = InventoryTarget(
                                        productID: reminder.productID,
                                        locationID: reminder.locationID,
                                        highlightBatchID: reminder.batchID,
                                    )
                                }
                                .buttonStyle(.borderedProminent)
                                Button("Acknowledge") {
                                    Task { await appState.acknowledgeReminder(id: reminder.id) }
                                }
                                .buttonStyle(.bordered)
                            }
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
        }
        .navigationTitle("Reminders")
        .refreshable {
            await appState.syncDueReminders(limit: 50)
        }
        .task {
            await appState.syncDueReminders(limit: 50)
        }
    }

    private static let localTime: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter
    }()
}
