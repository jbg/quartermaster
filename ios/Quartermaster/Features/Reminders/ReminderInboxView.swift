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
                if appState.isLoadingReminders && appState.reminders.isEmpty {
                    HStack {
                        Spacer()
                        ProgressView("Loading reminders…")
                            .padding(.vertical, 12)
                        Spacer()
                    }
                    .frame(maxWidth: .infinity)
                } else if let error = appState.reminderInboxError, appState.reminders.isEmpty {
                    ContentUnavailableView {
                        Label("Couldn't load reminders", systemImage: "exclamationmark.bubble")
                    } description: {
                        Text(error)
                    } actions: {
                        Button("Try again") {
                            Task { await appState.refreshRemindersAfterUserAction(limit: 50) }
                        }
                    }
                    .frame(maxWidth: .infinity)
                } else if appState.reminders.isEmpty {
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
                            if let schedule = scheduleText(for: reminder) {
                                Text(schedule)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            HStack {
                                Button("Open") {
                                    appState.openReminderFromInbox(reminder)
                                }
                                .buttonStyle(.borderedProminent)
                                .disabled(appState.isReminderActionInFlight(id: reminder.id))
                                Button("Acknowledge") {
                                    Task { await appState.acknowledgeReminder(id: reminder.id) }
                                }
                                .buttonStyle(.bordered)
                                .disabled(appState.isReminderActionInFlight(id: reminder.id))
                            }
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
        }
        .navigationTitle("Reminders")
        .refreshable {
            await appState.refreshRemindersAfterUserAction(limit: 50)
        }
        .task {
            await appState.loadReminderInbox(limit: 50)
        }
    }

    private func scheduleText(for reminder: Reminder) -> String? {
        if let household = reminder.householdFireLocalAtDate {
            let householdText = "Household time \(Self.formatHouseholdTime(household, timeZone: appState.householdTimeZone))"
            guard appState.timezonesDiffer, let local = reminder.fireAtDate else {
                return householdText
            }
            return "\(householdText) · Here \(Self.localTime.string(from: local))"
        }
        guard let local = reminder.fireAtDate else { return nil }
        return "Fires \(Self.localTime.string(from: local))"
    }

    private static func formatHouseholdTime(_ date: Date, timeZone: TimeZone?) -> String {
        householdTime.timeZone = timeZone
        return householdTime.string(from: date)
    }

    private static let householdTime: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter
    }()

    private static let localTime: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter
    }()
}
