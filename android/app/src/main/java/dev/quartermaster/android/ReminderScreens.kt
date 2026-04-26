package dev.quartermaster.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.ReminderDto
import kotlinx.coroutines.launch

@Composable
internal fun ReminderScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()

    LaunchedEffect(appState.currentHouseholdId) {
        appState.refreshReminders(limit = 50)
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.ReminderScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = "Reminders",
                subtitle = "Due expiry reminders stay here until someone opens or acknowledges them.",
            )
        }

        when {
            appState.remindersLoadState == LoadState.Loading && appState.reminders.isEmpty() -> {
                item {
                    InlineStatusCard(
                        title = "Loading reminders",
                        message = "Fetching the household inbox and marking unseen items as presented.",
                    )
                }
            }
            appState.reminderError != null && appState.reminders.isEmpty() -> {
                item {
                    ErrorCard(
                        title = "Couldn't load reminders",
                        message = appState.reminderError!!,
                        actionLabel = "Retry",
                        onAction = { scope.launch { appState.refreshReminders(limit = 50) } },
                    )
                }
            }
            appState.reminders.isEmpty() -> {
                item {
                    StatusCard(
                        title = "No due reminders",
                        message = "Expiry reminders stay here until someone opens or acknowledges them.",
                    )
                }
            }
        }
        if (appState.isRemindersRefreshing && appState.reminders.isNotEmpty()) {
            item {
                InlineStatusCard(
                    title = "Refreshing reminders",
                    message = "Syncing the latest due reminders for this household.",
                )
            }
        }
        appState.reminderError?.takeIf { appState.reminders.isNotEmpty() }?.let { message ->
            item {
                ErrorCard(
                    title = "Reminder action failed",
                    message = message,
                    actionLabel = "Refresh reminders",
                    onAction = { scope.launch { appState.refreshReminders(limit = 50) } },
                )
            }
        }

        items(appState.reminders, key = { it.id }) { reminder ->
            ReminderCard(
                reminder = reminder,
                action = appState.reminderActionFor(reminder.id.toString()),
                onOpen = { scope.launch { appState.openReminder(reminder) } },
                onAcknowledge = { scope.launch { appState.acknowledgeReminder(reminder.id.toString()) } },
            )
        }
    }
}

@Composable
private fun ReminderCard(
    reminder: ReminderDto,
    action: ReminderAction?,
    onOpen: () -> Unit,
    onAcknowledge: () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .testTag(SmokeTag.reminderCard(reminder.id.toString()))
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(reminder.title, style = MaterialTheme.typography.titleMedium)
            Text(reminder.body)
            Text(
                "Fires ${reminder.householdFireLocalAt} ${reminder.householdTimezone}",
                style = MaterialTheme.typography.bodySmall,
            )
            if (action != null) {
                InlineStatusCard(
                    title = "Updating reminder",
                    message =
                    if (action == ReminderAction.Open) {
                        "Opening reminder and refreshing inventory…"
                    } else {
                        "Acknowledging reminder and removing it from the inbox…"
                    },
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    modifier = Modifier.testTag(SmokeTag.reminderOpenButton(reminder.id.toString())),
                    onClick = onOpen,
                    enabled = action == null,
                ) {
                    Text(if (action == ReminderAction.Open) "Opening..." else "Open")
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.reminderAckButton(reminder.id.toString())),
                    onClick = onAcknowledge,
                    enabled = action == null,
                ) {
                    Text(if (action == ReminderAction.Acknowledge) "Acknowledging..." else "Acknowledge")
                }
            }
        }
    }
}
