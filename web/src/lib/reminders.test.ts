import { describe, expect, it } from 'vitest';
import {
  actionDone,
  loadReminders,
  optimisticAckRollback,
  optimisticAckStart,
  reminderBatchId
} from './reminders';

describe('reminder helpers', () => {
  it('loads reminders and presents only unpresented reminders', async () => {
    const presented: string[] = [];
    const state = await loadReminders({
      async remindersList() {
        return {
          items: [
            { id: 'reminder-1', title: 'Rice', body: 'Due', batch_id: 'batch-1' },
            {
              id: 'reminder-2',
              title: 'Beans',
              body: 'Due',
              batch_id: 'batch-2',
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
    expect(state.items).toHaveLength(2);
    expect(presented).toEqual(['reminder-1']);
  });

  it('removes optimistically and restores a reminder on ack rollback', () => {
    const reminder = { id: 'reminder-1', title: 'Rice', body: 'Due', batchId: 'batch-1' };
    const started = optimisticAckStart(
      { status: 'loaded', items: [reminder], error: null, actionIds: new Set() },
      reminder.id
    );

    expect(started.items).toEqual([]);
    expect(started.actionIds.has(reminder.id)).toBe(true);

    const rolledBack = optimisticAckRollback(started, reminder, 'Nope');
    expect(rolledBack.items).toEqual([reminder]);
    expect(rolledBack.error).toBe('Nope');
    expect(rolledBack.actionIds.has(reminder.id)).toBe(false);
  });

  it('normalizes generated reminder batch field names and clears action state', () => {
    const done = actionDone(
      { status: 'loaded', items: [], error: null, actionIds: new Set(['reminder-1']) },
      'reminder-1'
    );

    expect(reminderBatchId({ id: 'reminder-1', title: 'Rice', body: 'Due', batchId: 'batch-1' })).toBe('batch-1');
    expect(done.actionIds.has('reminder-1')).toBe(false);
  });
});
