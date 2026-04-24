import { describe, expect, it } from 'vitest';
import { isDepleted, loadInventory, stockExpiry, stockLocation, stockName, stockUnit } from './inventory';

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
  });
});
