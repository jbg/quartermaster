import { describe, expect, it } from 'vitest';
import {
  actionDone,
  formatReminderDate,
  formatReminderDateTime,
  loadReminders,
  optimisticAckRollback,
  optimisticAckStart,
  reminderBatchId,
  reminderBody,
  reminderExpiresOn,
  reminderFireAt,
  reminderTitle,
  reminderUrgency,
  sortReminders,
  startReminderAction
} from './reminders';
import type { Reminder } from './session-core';

function reminder(fields: Partial<Reminder> & { id: string }): Reminder {
  return {
    kind: 'expiry',
    batch_id: `${fields.id}-batch`,
    product_id: `${fields.id}-product`,
    location_id: `${fields.id}-location`,
    product_name: 'Rice',
    location_name: 'Pantry',
    quantity: '2',
    unit: 'kg',
    household_fire_local_at: '2026-04-23T09:00:00+02:00',
    fire_at: '2026-04-23T07:00:00.000Z',
    ...fields
  } as Reminder;
}

describe('reminder helpers', () => {
  it('loads reminders and presents only unpresented reminders', async () => {
    const presented: string[] = [];
    const state = await loadReminders({
      async remindersList() {
        return {
          items: [
            reminder({
              id: 'reminder-1',
              batch_id: 'batch-1',
              days_until_expiry: 1,
              urgency: 'expires_tomorrow',
              expires_on: '2026-04-25'
            }),
            reminder({
              id: 'reminder-2',
              batch_id: 'batch-2',
              days_until_expiry: 0,
              urgency: 'expires_today',
              expires_on: '2026-04-24',
              presented_on_device_at: '2026-04-24T00:00:00Z'
            })
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
    const first = reminder({
      id: 'reminder-1',
      batch_id: 'batch-1',
      days_until_expiry: 0,
      expires_on: '2026-04-24'
    });
    const later = reminder({
      id: 'reminder-2',
      batch_id: 'batch-2',
      days_until_expiry: 2,
      expires_on: '2026-04-26'
    });
    const started = optimisticAckStart(
      {
        status: 'loaded',
        items: [first, later],
        error: null,
        actionIds: new Set(),
        actionKinds: {}
      },
      first.id
    );

    expect(started.items).toEqual([later]);
    expect(started.actionIds.has(first.id)).toBe(true);
    expect(started.actionKinds[first.id]).toBe('ack');

    const rolledBack = optimisticAckRollback(started, first, 'Nope');
    expect(rolledBack.items.map((item) => item.id)).toEqual(['reminder-1', 'reminder-2']);
    expect(rolledBack.error).toBe('Nope');
    expect(rolledBack.actionIds.has(first.id)).toBe(false);
    expect(rolledBack.actionKinds[first.id]).toBeUndefined();
  });

  it('sorts reminders by server days, expiry, household fire time, then id', () => {
    const sorted = sortReminders([
      reminder({
        id: 'reminder-c',
        days_until_expiry: 2,
        expires_on: '2026-04-26',
        household_fire_local_at: '2026-04-25T09:00:00+02:00'
      }),
      reminder({
        id: 'reminder-b',
        days_until_expiry: 0,
        expires_on: '2026-04-24',
        household_fire_local_at: '2026-04-24T10:00:00+02:00'
      }),
      reminder({
        id: 'reminder-a',
        days_until_expiry: 0,
        expires_on: '2026-04-24',
        household_fire_local_at: '2026-04-24T09:00:00+02:00'
      }),
      reminder({
        id: 'reminder-d',
        household_fire_local_at: '2026-04-23T09:00:00+02:00'
      })
    ]);

    expect(sorted.map((item) => item.id)).toEqual([
      'reminder-a',
      'reminder-b',
      'reminder-c',
      'reminder-d'
    ]);
  });

  it('renders local reminder copy from semantic fields', () => {
    const item = reminder({
      id: 'reminder-1',
      product_name: 'Smoke Rice',
      location_name: 'Pantry',
      quantity: '2',
      unit: 'kg',
      expires_on: '2026-04-24',
      days_until_expiry: -2,
      urgency: 'expired'
    });

    expect(reminderTitle(item)).toBe('Smoke Rice in Pantry');
    expect(reminderBody(item)).toBe('2 kg expires on 2026-04-24.');
    expect(reminderUrgency(item)).toBe('Expired 2 days ago');
    expect(
      reminderUrgency(reminder({ id: 'today', urgency: 'expires_today', days_until_expiry: 0 }))
    ).toBe('Expires today');
    expect(
      reminderUrgency(
        reminder({ id: 'tomorrow', urgency: 'expires_tomorrow', days_until_expiry: 1 })
      )
    ).toBe('Expires tomorrow');
    expect(
      reminderUrgency(reminder({ id: 'future', urgency: 'expires_future', days_until_expiry: 4 }))
    ).toBe('Expires in 4 days');
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

    expect(reminderBatchId(reminder({ id: 'reminder-1', batch_id: 'batch-1' }))).toBe('batch-1');
    expect(
      reminderExpiresOn(
        reminder({
          id: 'reminder-1',
          expires_on: '2026-04-24'
        })
      )
    ).toBe('2026-04-24');
    expect(
      reminderFireAt(
        reminder({
          id: 'reminder-1',
          household_fire_local_at: '2026-04-23T09:00:00+02:00'
        })
      )
    ).toBe('2026-04-23T09:00:00+02:00');
    expect(done.actionIds.has('reminder-1')).toBe(false);
    expect(done.actionKinds['reminder-1']).toBeUndefined();
  });

  it('preserves fallback items on load failure and tracks action kind', async () => {
    const fallback = [reminder({ id: 'reminder-1', batch_id: 'batch-1' })];
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
