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
  return reminder.batch_id ?? reminder.batchId ?? '';
}

export function reminderProductId(reminder: Reminder): string {
  return reminder.product_id ?? reminder.productId ?? '';
}

export function reminderPresentedAt(reminder: Reminder): string | null {
  return reminder.presented_on_device_at ?? reminder.presentedOnDeviceAt ?? null;
}

export function reminderFireAt(reminder: Reminder): string {
  return (
    reminder.household_fire_local_at ??
    reminder.householdFireLocalAt ??
    reminder.fire_at ??
    reminder.fireAt ??
    ''
  );
}

export function reminderExpiresOn(reminder: Reminder): string {
  return reminder.expires_on ?? reminder.expiresOn ?? '';
}

export function sortReminders(reminders: Reminder[]): Reminder[] {
  return [...reminders].sort((a, b) => {
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

export function reminderUrgency(reminder: Reminder): string {
  const expiresOn = reminderExpiresOn(reminder);
  if (!expiresOn) {
    return 'Expiry date unavailable';
  }
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(expiresOn);
  if (!match) {
    return `Expires ${expiresOn}`;
  }
  const [, year, month, day] = match;
  const expires = new Date(Number(year), Number(month) - 1, Number(day));
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  expires.setHours(0, 0, 0, 0);
  const days = Math.round((expires.getTime() - today.getTime()) / 86_400_000);
  if (days < 0) {
    const count = Math.abs(days);
    return count === 1 ? 'Expired yesterday' : `Expired ${count} days ago`;
  }
  if (days === 0) {
    return 'Expires today';
  }
  if (days === 1) {
    return 'Expires tomorrow';
  }
  return `Expires in ${days} days`;
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
