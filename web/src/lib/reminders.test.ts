import { describe, expect, it } from 'vitest';
import {
  actionDone,
  formatReminderDate,
  formatReminderDateTime,
  loadReminders,
  optimisticAckRollback,
  optimisticAckStart,
  reminderBatchId,
  reminderExpiresOn,
  reminderFireAt,
  reminderUrgency,
  sortReminders,
  startReminderAction
} from './reminders';

describe('reminder helpers', () => {
  it('loads reminders and presents only unpresented reminders', async () => {
    const presented: string[] = [];
    const state = await loadReminders({
      async remindersList() {
        return {
          items: [
            {
              id: 'reminder-1',
              title: 'Rice',
              body: 'Due',
              batch_id: 'batch-1',
              expires_on: '2026-04-25'
            },
            {
              id: 'reminder-2',
              title: 'Beans',
              body: 'Due',
              batch_id: 'batch-2',
              expires_on: '2026-04-24',
              presented_on_device_at: '2026-04-24T00:00:00Z'
            }
          ]
        };
      },
      async remindersPresent(id: string) {
        presented.push(id);
      }
    });

    expect(state.status).toBe('loaded');
    expect(state.items.map((item) => item.id)).toEqual(['reminder-2', 'reminder-1']);
    expect(presented).toEqual(['reminder-1']);
  });

  it('removes optimistically and restores a reminder on ack rollback in sorted order', () => {
    const reminder = {
      id: 'reminder-1',
      title: 'Rice',
      body: 'Due',
      batchId: 'batch-1',
      expiresOn: '2026-04-24'
    };
    const later = {
      id: 'reminder-2',
      title: 'Beans',
      body: 'Due',
      batchId: 'batch-2',
      expiresOn: '2026-04-26'
    };
    const started = optimisticAckStart(
      {
        status: 'loaded',
        items: [reminder, later],
        error: null,
        actionIds: new Set(),
        actionKinds: {}
      },
      reminder.id
    );

    expect(started.items).toEqual([later]);
    expect(started.actionIds.has(reminder.id)).toBe(true);
    expect(started.actionKinds[reminder.id]).toBe('ack');

    const rolledBack = optimisticAckRollback(started, reminder, 'Nope');
    expect(rolledBack.items.map((item) => item.id)).toEqual(['reminder-1', 'reminder-2']);
    expect(rolledBack.error).toBe('Nope');
    expect(rolledBack.actionIds.has(reminder.id)).toBe(false);
    expect(rolledBack.actionKinds[reminder.id]).toBeUndefined();
  });

  it('sorts reminders by expiry, household fire time, then id', () => {
    const sorted = sortReminders([
      {
        id: 'reminder-c',
        title: 'C',
        body: 'Due',
        expiresOn: '2026-04-26',
        householdFireLocalAt: '2026-04-25T09:00:00+02:00'
      },
      {
        id: 'reminder-b',
        title: 'B',
        body: 'Due',
        expiresOn: '2026-04-24',
        householdFireLocalAt: '2026-04-24T10:00:00+02:00'
      },
      {
        id: 'reminder-a',
        title: 'A',
        body: 'Due',
        expiresOn: '2026-04-24',
        householdFireLocalAt: '2026-04-24T09:00:00+02:00'
      },
      {
        id: 'reminder-d',
        title: 'D',
        body: 'Due',
        householdFireLocalAt: '2026-04-23T09:00:00+02:00'
      }
    ]);

    expect(sorted.map((item) => item.id)).toEqual([
      'reminder-a',
      'reminder-b',
      'reminder-c',
      'reminder-d'
    ]);
  });

  it('describes reminder urgency from expiry dates', () => {
    const today = new Date();
    const todayText = [
      today.getFullYear(),
      String(today.getMonth() + 1).padStart(2, '0'),
      String(today.getDate()).padStart(2, '0')
    ].join('-');

    expect(reminderUrgency({ id: 'reminder-1', title: 'Rice', body: 'Due' })).toBe(
      'Expiry date unavailable'
    );
    expect(
      reminderUrgency({ id: 'reminder-2', title: 'Rice', body: 'Due', expiresOn: todayText })
    ).toBe('Expires today');
  });

  it('normalizes generated reminder batch field names and clears action state', () => {
    const done = actionDone(
      {
        status: 'loaded',
        items: [],
        error: null,
        actionIds: new Set(['reminder-1']),
        actionKinds: { 'reminder-1': 'open' }
      },
      'reminder-1'
    );

    expect(
      reminderBatchId({ id: 'reminder-1', title: 'Rice', body: 'Due', batchId: 'batch-1' })
    ).toBe('batch-1');
    expect(
      reminderExpiresOn({
        id: 'reminder-1',
        title: 'Rice',
        body: 'Due',
        expiresOn: '2026-04-24'
      })
    ).toBe('2026-04-24');
    expect(
      reminderFireAt({
        id: 'reminder-1',
        title: 'Rice',
        body: 'Due',
        householdFireLocalAt: '2026-04-23T09:00:00+02:00'
      })
    ).toBe('2026-04-23T09:00:00+02:00');
    expect(done.actionIds.has('reminder-1')).toBe(false);
    expect(done.actionKinds['reminder-1']).toBeUndefined();
  });

  it('preserves fallback items on load failure and tracks action kind', async () => {
    const fallback = [{ id: 'reminder-1', title: 'Rice', body: 'Due', batch_id: 'batch-1' }];
    const state = await loadReminders(
      {
        async remindersList() {
          throw new Error('offline');
        },
        async remindersPresent() {
          throw new Error('unused');
        }
      },
      new Set(['reminder-1']),
      { 'reminder-1': 'open' },
      fallback
    );

    expect(state.status).toBe('error');
    expect(state.items).toEqual(fallback);
    expect(state.actionIds.has('reminder-1')).toBe(true);
    expect(state.actionKinds['reminder-1']).toBe('open');

    const started = startReminderAction(state, 'reminder-2', 'ack');
    expect(started.actionKinds['reminder-2']).toBe('ack');
  });

  it('formats reminder dates with raw fallback', () => {
    expect(formatReminderDate('not-a-date')).toBe('not-a-date');
    expect(formatReminderDateTime('not-a-date')).toBe('not-a-date');
    expect(formatReminderDate('2026-04-24')).not.toBe('2026-04-24');
    expect(formatReminderDateTime('2026-04-23T09:00:00+02:00')).not.toBe(
      '2026-04-23T09:00:00+02:00'
    );
  });
});
