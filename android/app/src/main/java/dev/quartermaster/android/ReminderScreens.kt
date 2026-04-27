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
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.ReminderUrgency
import kotlinx.coroutines.launch
import java.time.LocalDate
import java.time.OffsetDateTime
import java.time.format.DateTimeFormatter
import java.util.Locale

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
                title = stringResource(R.string.reminder_inbox_title),
                subtitle = stringResource(R.string.reminder_inbox_subtitle),
            )
        }

        when {
            appState.remindersLoadState == LoadState.Loading && appState.reminders.isEmpty() -> {
                item {
                    InlineStatusCard(
                        title = stringResource(R.string.reminder_inbox_loading_title),
                        message = stringResource(R.string.reminder_inbox_loading_body),
                    )
                }
            }
            appState.reminderError != null && appState.reminders.isEmpty() -> {
                item {
                    ErrorCard(
                        title = stringResource(R.string.reminder_inbox_load_error_title),
                        message = appState.reminderError!!,
                        actionLabel = stringResource(R.string.reminder_inbox_retry),
                        onAction = { scope.launch { appState.refreshReminders(limit = 50) } },
                    )
                }
            }
            appState.reminders.isEmpty() -> {
                item {
                    StatusCard(
                        title = stringResource(R.string.reminder_inbox_empty_title),
                        message = stringResource(R.string.reminder_inbox_empty_body),
                    )
                }
            }
        }
        if (appState.isRemindersRefreshing && appState.reminders.isNotEmpty()) {
            item {
                InlineStatusCard(
                    title = stringResource(R.string.reminder_inbox_refreshing_title),
                    message = stringResource(R.string.reminder_inbox_refreshing_body),
                )
            }
        }
        appState.reminderError?.takeIf { appState.reminders.isNotEmpty() }?.let { message ->
            item {
                ErrorCard(
                    title = stringResource(R.string.reminder_inbox_action_error_title),
                    message = message,
                    actionLabel = stringResource(R.string.reminder_inbox_refresh_action),
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
    val title = reminderDisplayTitle(reminder)
    val body = reminderDisplayBody(reminder)
    val urgency = reminderUrgencyText(reminder)
    Card {
        Column(
            modifier = Modifier
                .testTag(SmokeTag.reminderCard(reminder.id.toString()))
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(body)
            urgency?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            reminder.expiresOn?.let { expiresOn ->
                Text(
                    stringResource(R.string.reminder_inbox_expiry_date, formatReminderDate(expiresOn)),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(
                stringResource(
                    R.string.reminder_inbox_household_time,
                    formatReminderDateTime(reminder.householdFireLocalAt),
                    reminder.householdTimezone,
                ),
                style = MaterialTheme.typography.bodySmall,
            )
            if (action != null) {
                InlineStatusCard(
                    title = stringResource(R.string.reminder_inbox_updating_title),
                    message =
                    if (action == ReminderAction.Open) {
                        stringResource(R.string.reminder_inbox_opening_body)
                    } else {
                        stringResource(R.string.reminder_inbox_acknowledging_body)
                    },
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    modifier = Modifier.testTag(SmokeTag.reminderOpenButton(reminder.id.toString())),
                    onClick = onOpen,
                    enabled = action == null,
                ) {
                    Text(
                        if (action == ReminderAction.Open) {
                            stringResource(R.string.reminder_inbox_opening_action)
                        } else {
                            stringResource(R.string.reminder_inbox_open)
                        },
                    )
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.reminderAckButton(reminder.id.toString())),
                    onClick = onAcknowledge,
                    enabled = action == null,
                ) {
                    Text(
                        if (action == ReminderAction.Acknowledge) {
                            stringResource(R.string.reminder_inbox_acknowledging_action)
                        } else {
                            stringResource(R.string.reminder_inbox_acknowledge)
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun reminderDisplayTitle(reminder: ReminderDto): String = stringResource(R.string.expiry_reminder_title, reminder.productName, reminder.locationName)

@Composable
private fun reminderDisplayBody(reminder: ReminderDto): String = reminder.expiresOn?.let { expiresOn ->
    stringResource(R.string.expiry_reminder_body, reminder.quantity, reminder.unit, expiresOn)
} ?: stringResource(R.string.expiry_reminder_body_no_date, reminder.quantity, reminder.unit)

@Composable
private fun reminderUrgencyText(reminder: ReminderDto): String? {
    val urgency = reminder.urgency ?: return null
    val days = reminder.daysUntilExpiry
    return when (urgency) {
        ReminderUrgency.EXPIRED -> {
            val count = days?.let { -it }
            when (count) {
                1L -> stringResource(R.string.expiry_reminder_urgency_expired_yesterday)
                null -> stringResource(R.string.expiry_reminder_urgency_expired)
                else -> pluralStringResource(
                    R.plurals.expiry_reminder_urgency_expired_days_ago,
                    count.toInt(),
                    count,
                )
            }
        }
        ReminderUrgency.EXPIRES_TODAY -> stringResource(R.string.expiry_reminder_urgency_today)
        ReminderUrgency.EXPIRES_TOMORROW -> stringResource(R.string.expiry_reminder_urgency_tomorrow)
        ReminderUrgency.EXPIRES_FUTURE -> days?.let {
            pluralStringResource(R.plurals.expiry_reminder_urgency_future_days, it.toInt(), it)
        } ?: stringResource(R.string.expiry_reminder_urgency_soon)
    }
}

internal fun formatReminderDate(value: String): String = runCatching {
    LocalDate.parse(value).format(reminderDateFormatter)
}.getOrDefault(value)

internal fun formatReminderDateTime(value: String): String = runCatching {
    OffsetDateTime.parse(value).format(reminderDateTimeFormatter)
}.getOrDefault(value)

private val reminderDateFormatter =
    DateTimeFormatter.ofPattern("MMM d, yyyy", Locale.getDefault())

private val reminderDateTimeFormatter =
    DateTimeFormatter.ofPattern("MMM d, yyyy, h:mm a", Locale.getDefault())
