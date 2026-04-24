import { describe, expect, it } from 'vitest';
import {
  canRestoreBatch,
  isDepleted,
  loadInventory,
  selectBatchAfterRefresh,
  stockExpiry,
  stockLocation,
  stockName,
  stockUnit
} from './inventory';

describe('inventory helpers', () => {
  it('loads stock into a loaded state', async () => {
    const state = await loadInventory({
      async stockList() {
        return {
      items: [{ id: 'batch-1', product: { name: 'Flour' }, quantity: '1.5', unit: 'kg' }]
        };
      }
    });

    expect(state.status).toBe('loaded');
    expect(state.items).toHaveLength(1);
  });

  it('returns an error state when loading fails', async () => {
    const state = await loadInventory({
      async stockList() {
        throw new Error('boom');
      }
    });

    expect(state.status).toBe('error');
    expect(state.items).toEqual([]);
  });

  it('formats mixed generated field names defensively', () => {
    const batch = {
      id: 'batch-1',
      productName: 'Rice',
      unitCode: 'g',
      locationName: null,
      expiresOn: '2026-05-01',
      depletedAt: '2026-04-01T00:00:00Z'
    };

    expect(stockName(batch)).toBe('Rice');
    expect(stockUnit(batch)).toBe('g');
    expect(stockLocation(batch)).toBe('No location');
    expect(stockExpiry(batch)).toBe('2026-05-01');
    expect(isDepleted(batch)).toBe(true);
    expect(isDepleted({ id: 'batch-2', product: { name: 'Flour' }, quantity: '0' })).toBe(true);
  });

  it('gates restore to depleted batches whose latest event is discard', () => {
    const batch = {
      id: 'batch-1',
      product: { name: 'Rice' },
      depleted_at: '2026-04-01T00:00:00Z'
    };

    expect(canRestoreBatch(batch, [{ id: 'event-1', event_type: 'discard' }])).toBe(true);
    expect(canRestoreBatch(batch, [{ id: 'event-2', event_type: 'consume' }])).toBe(false);
    expect(canRestoreBatch({ ...batch, depleted_at: null }, [{ id: 'event-1', event_type: 'discard' }])).toBe(false);
  });

  it('keeps the preferred selection after inventory refresh when possible', () => {
    const items = [
      { id: 'batch-1', product: { name: 'Rice' } },
      { id: 'batch-2', product: { name: 'Beans' } }
    ];

    expect(selectBatchAfterRefresh(items, 'batch-2')?.id).toBe('batch-2');
    expect(selectBatchAfterRefresh(items, 'missing')?.id).toBe('batch-1');
    expect(selectBatchAfterRefresh([], 'batch-2')).toBeNull();
  });
});
