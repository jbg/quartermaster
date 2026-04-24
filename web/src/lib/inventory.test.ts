import { describe, expect, it } from 'vitest';
import {
  canRestoreBatch,
  isDepleted,
  loadInventory,
  productPreferredUnit,
  productSource,
  selectBatchAfterRefresh,
  stockExpiry,
  stockLocation,
  stockName,
  stockUnit,
  unitChoicesForFamily,
  validateAddStockInput
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
    expect(
      canRestoreBatch({ ...batch, depleted_at: null }, [{ id: 'event-1', event_type: 'discard' }])
    ).toBe(false);
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

  it('normalizes product display helpers and unit choices', () => {
    expect(unitChoicesForFamily('mass')).toEqual(['g', 'kg']);
    expect(unitChoicesForFamily('volume')).toEqual(['ml', 'l']);
    expect(unitChoicesForFamily('count')).toEqual(['piece']);
    expect(productPreferredUnit({ id: 'product-1', name: 'Rice', family: 'mass' })).toBe('g');
    expect(
      productPreferredUnit({ id: 'product-2', name: 'Milk', family: 'volume', preferredUnit: 'l' })
    ).toBe('l');
    expect(
      productSource({ id: 'product-3', name: 'Beans', family: 'count', source: 'manual' })
    ).toBe('Manual');
    expect(
      productSource({
        id: 'product-4',
        name: 'Pasta',
        family: 'mass',
        source: 'openfoodfacts'
      })
    ).toBe('OpenFoodFacts');
  });

  it('validates add-stock inputs before submit', () => {
    const product = { id: 'product-1', name: 'Rice', family: 'mass' as const };

    expect(validateAddStockInput({ product: null, quantity: '1', locationId: 'pantry' })).toBe(
      'Select or create a product first.'
    );
    expect(validateAddStockInput({ product, quantity: '1', locationId: '' })).toBe(
      'Choose a location before adding stock.'
    );
    expect(validateAddStockInput({ product, quantity: '0', locationId: 'pantry' })).toBe(
      'Enter a positive stock quantity.'
    );
    expect(validateAddStockInput({ product, quantity: '2.5', locationId: 'pantry' })).toBeNull();
  });
});
