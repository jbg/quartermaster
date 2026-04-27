import type { Reminder } from './session-core';
import type { ReminderActionKind } from './reminders';

const dateFormatter = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });
const dateTimeFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short'
});
const pluralRules = new Intl.PluralRules(undefined, { type: 'cardinal' });

export const reminderMessages = {
  headingEyebrow: 'Due now',
  headingTitle: 'Reminders',
  loading: 'Loading reminders...',
  refreshing: 'Refreshing reminders...',
  empty: 'No due reminders.',
  loadError: 'Reminders could not be loaded.',
  openError: 'Reminder could not be opened.',
  ackError: 'Reminder could not be acknowledged.',
  expiryReminder: 'Expiry reminder',
  expiryDateLabel: 'Expiry date',
  householdTimeLabel: 'Household time',
  openAction: 'Open',
  ackAction: 'Ack',
  openingAction: 'Opening...',
  acknowledgingAction: 'Acknowledging...',
  openingStatus: 'Opening reminder...',
  acknowledgingStatus: 'Acknowledging reminder...'
};

export function reminderActionLabel(action: ReminderActionKind | undefined, base: 'open' | 'ack') {
  if (base === 'open') {
    return action === 'open' ? reminderMessages.openingAction : reminderMessages.openAction;
  }
  return action === 'ack' ? reminderMessages.acknowledgingAction : reminderMessages.ackAction;
}

export function reminderActionStatus(action: ReminderActionKind | undefined): string | null {
  if (action === 'open') {
    return reminderMessages.openingStatus;
  }
  if (action === 'ack') {
    return reminderMessages.acknowledgingStatus;
  }
  return null;
}

export function reminderTitleText(reminder: Reminder): string {
  return `${reminder.product_name} in ${reminder.location_name}`;
}

export function reminderBodyText(reminder: Reminder): string {
  const expiry = reminder.expires_on ?? '';
  const quantity = `${reminder.quantity} ${reminder.unit}`;
  return expiry ? `${quantity} expires on ${expiry}.` : `${quantity} has an expiry reminder.`;
}

export function reminderUrgencyText(reminder: Reminder): string {
  const days = reminder.days_until_expiry ?? null;
  switch (reminder.urgency) {
    case 'expired': {
      if (days === -1) {
        return 'Expired yesterday';
      }
      const count = days == null ? 0 : Math.abs(days);
      return count > 1 ? `Expired ${count} ${dayWord(count)} ago` : 'Expired';
    }
    case 'expires_today':
      return 'Expires today';
    case 'expires_tomorrow':
      return 'Expires tomorrow';
    case 'expires_future':
      return days == null ? 'Expires soon' : `Expires in ${days} ${dayWord(days)}`;
    default:
      return 'Expiry date unavailable';
  }
}

export function formatReminderDateText(value: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) {
    return value;
  }
  const [, year, month, day] = match;
  const parsed = new Date(Number(year), Number(month) - 1, Number(day));
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return dateFormatter.format(parsed);
}

export function formatReminderDateTimeText(value: string): string {
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) {
    return value;
  }
  return dateTimeFormatter.format(new Date(parsed));
}

function dayWord(count: number): string {
  return pluralRules.select(count) === 'one' ? 'day' : 'days';
}
