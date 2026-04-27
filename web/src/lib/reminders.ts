import type { QuartermasterSession, Reminder } from './session-core';

export type ReminderActionKind = 'open' | 'ack';

export interface ReminderState {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  items: Reminder[];
  error: string | null;
  actionIds: Set<string>;
  actionKinds: Record<string, ReminderActionKind>;
}

export const emptyReminderState: ReminderState = {
  status: 'idle',
  items: [],
  error: null,
  actionIds: new Set(),
  actionKinds: {}
};

export async function loadReminders(
  session: Pick<QuartermasterSession, 'remindersList' | 'remindersPresent'>,
  existingActionIds = new Set<string>(),
  existingActionKinds: Record<string, ReminderActionKind> = {},
  fallbackItems: Reminder[] = []
): Promise<ReminderState> {
  try {
    const response = await session.remindersList({ limit: 50 });
    const items = sortReminders(response.items ?? []);
    await Promise.all(
      items
        .filter((reminder) => !reminderPresentedAt(reminder) && !existingActionIds.has(reminder.id))
        .map((reminder) => session.remindersPresent(reminder.id).catch(() => undefined))
    );
    return {
      status: 'loaded',
      items,
      error: null,
      actionIds: new Set(existingActionIds),
      actionKinds: { ...existingActionKinds }
    };
  } catch {
    return {
      status: 'error',
      items: fallbackItems,
      error: 'Reminders could not be loaded.',
      actionIds: new Set(existingActionIds),
      actionKinds: { ...existingActionKinds }
    };
  }
}

export function reminderBatchId(reminder: Reminder): string {
  return reminder.batch_id;
}

export function reminderProductId(reminder: Reminder): string {
  return reminder.product_id;
}

export function reminderPresentedAt(reminder: Reminder): string | null {
  return reminder.presented_on_device_at ?? null;
}

export function reminderFireAt(reminder: Reminder): string {
  return reminder.household_fire_local_at;
}

export function reminderExpiresOn(reminder: Reminder): string {
  return reminder.expires_on ?? '';
}

export function sortReminders(reminders: Reminder[]): Reminder[] {
  return [...reminders].sort((a, b) => {
    const days = compareNumbers(reminderDaysUntilExpiry(a), reminderDaysUntilExpiry(b));
    if (days !== 0) {
      return days;
    }
    const expires = compareStrings(reminderExpiresOn(a), reminderExpiresOn(b));
    if (expires !== 0) {
      return expires;
    }
    const fireAt = compareStrings(reminderFireAt(a), reminderFireAt(b));
    if (fireAt !== 0) {
      return fireAt;
    }
    return a.id.localeCompare(b.id);
  });
}

export function reminderTitle(reminder: Reminder): string {
  return `${reminderProductName(reminder)} in ${reminderLocationName(reminder)}`;
}

export function reminderBody(reminder: Reminder): string {
  const expiry = reminderExpiresOn(reminder);
  const suffix = expiry ? ` expires on ${expiry}` : ' has an expiry reminder';
  return `${reminder.quantity} ${reminder.unit}${suffix}.`;
}

export function reminderUrgency(reminder: Reminder): string {
  const days = reminderDaysUntilExpiry(reminder);
  switch (reminder.urgency) {
    case 'expired': {
      if (days === -1) {
        return 'Expired yesterday';
      }
      const count = days == null ? 0 : Math.abs(days);
      return count > 1 ? `Expired ${count} days ago` : 'Expired';
    }
    case 'expires_today':
      return 'Expires today';
    case 'expires_tomorrow':
      return 'Expires tomorrow';
    case 'expires_future':
      return days == null ? 'Expires soon' : `Expires in ${days} days`;
    default:
      return 'Expiry date unavailable';
  }
}

function reminderDaysUntilExpiry(reminder: Reminder): number | null {
  return reminder.days_until_expiry ?? null;
}

function reminderProductName(reminder: Reminder): string {
  return reminder.product_name;
}

function reminderLocationName(reminder: Reminder): string {
  return reminder.location_name;
}

function compareNumbers(a: number | null, b: number | null): number {
  if (a == null && b == null) {
    return 0;
  }
  if (a == null) {
    return 1;
  }
  if (b == null) {
    return -1;
  }
  return a - b;
}

export function formatReminderDate(value: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) {
    return value;
  }
  const [, year, month, day] = match;
  const parsed = new Date(Number(year), Number(month) - 1, Number(day));
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(parsed);
}

export function formatReminderDateTime(value: string): string {
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) {
    return value;
  }
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short'
  }).format(new Date(parsed));
}

export function startReminderAction(
  state: ReminderState,
  id: string,
  action: ReminderActionKind
): ReminderState {
  const actionIds = new Set(state.actionIds);
  actionIds.add(id);
  return {
    ...state,
    error: null,
    actionIds,
    actionKinds: { ...state.actionKinds, [id]: action }
  };
}

export function optimisticAckStart(state: ReminderState, id: string): ReminderState {
  const started = startReminderAction(state, id, 'ack');
  return {
    ...started,
    items: started.items.filter((reminder) => reminder.id !== id)
  };
}

export function optimisticAckRollback(
  state: ReminderState,
  reminder: Reminder,
  message: string
): ReminderState {
  const actionIds = new Set(state.actionIds);
  actionIds.delete(reminder.id);
  const { [reminder.id]: _removed, ...actionKinds } = state.actionKinds;
  const items = state.items.some((item) => item.id === reminder.id)
    ? state.items
    : sortReminders([reminder, ...state.items]);
  return {
    ...state,
    items,
    error: message,
    actionIds,
    actionKinds
  };
}

function compareStrings(a: string, b: string): number {
  if (!a && !b) {
    return 0;
  }
  if (!a) {
    return 1;
  }
  if (!b) {
    return -1;
  }
  return a.localeCompare(b);
}

export function actionDone(state: ReminderState, id: string): ReminderState {
  const actionIds = new Set(state.actionIds);
  actionIds.delete(id);
  const { [id]: _removed, ...actionKinds } = state.actionKinds;
  return {
    ...state,
    actionIds,
    actionKinds
  };
}
