import type { QuartermasterSession, Reminder } from './session-core';

export interface ReminderState {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  items: Reminder[];
  error: string | null;
  actionIds: Set<string>;
}

export const emptyReminderState: ReminderState = {
  status: 'idle',
  items: [],
  error: null,
  actionIds: new Set()
};

export async function loadReminders(
  session: Pick<QuartermasterSession, 'remindersList' | 'remindersPresent'>,
  existingActionIds = new Set<string>()
): Promise<ReminderState> {
  try {
    const response = await session.remindersList({ limit: 50 });
    const items = response.items ?? [];
    await Promise.all(
      items
        .filter((reminder) => !reminderPresentedAt(reminder) && !existingActionIds.has(reminder.id))
        .map((reminder) => session.remindersPresent(reminder.id).catch(() => undefined))
    );
    return {
      status: 'loaded',
      items,
      error: null,
      actionIds: new Set(existingActionIds)
    };
  } catch {
    return {
      status: 'error',
      items: [],
      error: 'Reminders could not be loaded.',
      actionIds: new Set(existingActionIds)
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

export function optimisticAckStart(state: ReminderState, id: string): ReminderState {
  const actionIds = new Set(state.actionIds);
  actionIds.add(id);
  return {
    ...state,
    items: state.items.filter((reminder) => reminder.id !== id),
    error: null,
    actionIds
  };
}

export function optimisticAckRollback(
  state: ReminderState,
  reminder: Reminder,
  message: string
): ReminderState {
  const actionIds = new Set(state.actionIds);
  actionIds.delete(reminder.id);
  const items = state.items.some((item) => item.id === reminder.id)
    ? state.items
    : [reminder, ...state.items];
  return {
    ...state,
    items,
    error: message,
    actionIds
  };
}

export function actionDone(state: ReminderState, id: string): ReminderState {
  const actionIds = new Set(state.actionIds);
  actionIds.delete(id);
  return {
    ...state,
    actionIds
  };
}
