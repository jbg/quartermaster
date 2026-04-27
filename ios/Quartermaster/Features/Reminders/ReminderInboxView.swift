import SwiftUI

struct ReminderInboxView: View {
  @Environment(AppState.self) private var appState

  var body: some View {
    List {
      if appState.timezonesDiffer {
        Section {
          VStack(alignment: .leading, spacing: 6) {
            Text(NSLocalizedString("REMINDER_INBOX_TIMEZONE_TITLE", comment: ""))
              .font(.subheadline.weight(.semibold))
            Text(
              String(
                format: NSLocalizedString("REMINDER_INBOX_TIMEZONE_BODY", comment: ""),
                appState.householdTimeZoneID
                  ?? NSLocalizedString("REMINDER_INBOX_TIMEZONE_FALLBACK", comment: ""),
                appState.deviceTimeZone.identifier
              )
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
            ProgressView(NSLocalizedString("REMINDER_INBOX_LOADING", comment: ""))
              .padding(.vertical, 12)
            Spacer()
          }
          .frame(maxWidth: .infinity)
        } else if let error = appState.reminderInboxError, appState.reminders.isEmpty {
          ContentUnavailableView {
            Label(
              NSLocalizedString("REMINDER_INBOX_LOAD_ERROR_TITLE", comment: ""),
              systemImage: "exclamationmark.bubble"
            )
          } description: {
            Text(error)
          } actions: {
            Button(NSLocalizedString("REMINDER_INBOX_RETRY", comment: "")) {
              Task { await appState.refreshRemindersAfterUserAction(limit: 50) }
            }
          }
          .frame(maxWidth: .infinity)
        } else if appState.reminders.isEmpty {
          ContentUnavailableView(
            NSLocalizedString("REMINDER_INBOX_EMPTY_TITLE", comment: ""),
            systemImage: "bell.slash",
            description: Text(NSLocalizedString("REMINDER_INBOX_EMPTY_BODY", comment: ""))
          )
          .frame(maxWidth: .infinity)
        } else {
          ForEach(appState.reminders) { reminder in
            VStack(alignment: .leading, spacing: 8) {
              Text(reminder.displayTitle)
                .font(.headline)
              Text(reminder.displayBody)
                .font(.subheadline)
                .foregroundStyle(.secondary)
              if let expiresOn = reminder.expiresOn {
                Text(
                  appState.reminderUrgencyText(for: reminder)
                    ?? NSLocalizedString("REMINDER_INBOX_EXPIRY_REMINDER", comment: "")
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                Text(
                  String(
                    format: NSLocalizedString("REMINDER_INBOX_EXPIRY_DATE", comment: ""),
                    appState.displayDate(for: expiresOn) ?? expiresOn
                  )
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
              }
              if let schedule = scheduleText(for: reminder) {
                Text(schedule)
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
              if appState.isReminderActionInFlight(id: reminder.id) {
                HStack(spacing: 8) {
                  ProgressView()
                  Text(NSLocalizedString("REMINDER_INBOX_UPDATING", comment: ""))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
              }
              HStack {
                Button(NSLocalizedString("REMINDER_INBOX_OPEN", comment: "")) {
                  appState.openReminderFromInbox(reminder)
                }
                .buttonStyle(.borderedProminent)
                .disabled(appState.isReminderActionInFlight(id: reminder.id))
                Button(NSLocalizedString("REMINDER_INBOX_ACKNOWLEDGE", comment: "")) {
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
    .navigationTitle(NSLocalizedString("REMINDER_INBOX_TITLE", comment: ""))
    .refreshable {
      await appState.refreshRemindersAfterUserAction(limit: 50)
    }
    .task {
      await appState.loadReminderInbox(limit: 50)
    }
  }

  private func scheduleText(for reminder: Reminder) -> String? {
    if let household = reminder.householdFireLocalAtDate {
      let householdText =
        String(
          format: NSLocalizedString("REMINDER_INBOX_HOUSEHOLD_TIME", comment: ""),
          Self.formatHouseholdTime(household, timeZone: appState.householdTimeZone)
        )
      guard appState.timezonesDiffer, let local = reminder.fireAtDate else {
        return householdText
      }
      return String(
        format: NSLocalizedString("REMINDER_INBOX_HOUSEHOLD_AND_LOCAL_TIME", comment: ""),
        householdText,
        Self.localTime.string(from: local)
      )
    }
    guard let local = reminder.fireAtDate else { return nil }
    return String(
      format: NSLocalizedString("REMINDER_INBOX_FIRES_LOCAL_TIME", comment: ""),
      Self.localTime.string(from: local)
    )
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
