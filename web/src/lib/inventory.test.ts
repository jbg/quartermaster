import { describe, expect, it } from 'vitest';
import {
  buildStockUpdateRequest,
  canRestoreBatch,
  chooseBestBatch,
  groupInventoryByLocation,
  groupInventory,
  isDepleted,
  loadInventory,
  matchesInventorySearch,
  productPreferredUnit,
  productSource,
  selectBatchAfterRefresh,
  stockEditFields,
  stockDepletedAt,
  stockExpiry,
  stockLocation,
  stockName,
  stockUnit,
  unitChoicesForFamily,
  validateAddStockInput,
  validateStockEditInput
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
    expect(isDepleted({ id: 'batch-2', product: { name: 'Flour' }, quantity: '0' })).toBe(false);
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

  it('partitions active and depleted inventory without reordering rows', () => {
    const active = { id: 'batch-1', product: { name: 'Rice' } };
    const depleted = {
      id: 'batch-2',
      product: { name: 'Beans' },
      depletedAt: '2026-04-01T00:00:00Z'
    };
    const groups = groupInventory([depleted, active]);

    expect(groups.active).toEqual([active]);
    expect(groups.depleted).toEqual([depleted]);
    expect(stockDepletedAt(depleted)).toBe('2026-04-01T00:00:00Z');
  });

  it('groups visible inventory by sorted locations and products', () => {
    const locations = [
      { id: 'pantry', name: 'Pantry', sort_order: 0 },
      { id: 'fridge', name: 'Fridge', sort_order: 1 }
    ];
    const items = [
      {
        id: 'batch-1',
        product: { id: 'rice', name: 'Rice' },
        location_id: 'pantry',
        quantity: '2',
        unit: 'kg',
        expires_on: '2026-05-01'
      },
      {
        id: 'batch-2',
        product: { id: 'rice', name: 'Rice' },
        location_id: 'pantry',
        quantity: '3',
        unit: 'kg',
        expires_on: '2026-05-03'
      },
      {
        id: 'batch-3',
        product: { id: 'milk', name: 'Milk' },
        location_id: 'fridge',
        quantity: '1',
        unit: 'l',
        expires_on: '2026-04-29'
      }
    ];

    const groups = groupInventoryByLocation({
      items,
      locations,
      filter: 'active',
      search: '',
      today: new Date('2026-04-27T12:00:00')
    });

    expect(groups.map((group) => group.location.name)).toEqual(['Pantry', 'Fridge']);
    expect(groups[0].activeCount).toBe(2);
    expect(groups[0].productGroups).toHaveLength(1);
    expect(groups[0].productGroups[0]).toMatchObject({
      productName: 'Rice',
      totalQuantity: '5',
      totalUnit: 'kg',
      earliestExpiry: '2026-05-01'
    });
  });

  it('filters active depleted expired and soon inventory', () => {
    const locations = [{ id: 'pantry', name: 'Pantry' }];
    const items = [
      {
        id: 'active',
        product: { id: 'rice', name: 'Rice' },
        location_id: 'pantry',
        quantity: '1',
        unit: 'kg',
        expires_on: '2026-05-01'
      },
      {
        id: 'expired',
        product: { id: 'beans', name: 'Beans' },
        location_id: 'pantry',
        quantity: '1',
        unit: 'kg',
        expires_on: '2026-04-26'
      },
      {
        id: 'later',
        product: { id: 'pasta', name: 'Pasta' },
        location_id: 'pantry',
        quantity: '1',
        unit: 'kg',
        expires_on: '2026-05-10'
      },
      {
        id: 'depleted',
        product: { id: 'flour', name: 'Flour' },
        location_id: 'pantry',
        quantity: '0',
        unit: 'kg',
        depleted_at: '2026-04-20T00:00:00Z',
        expires_on: '2026-04-25'
      }
    ];

    const grouped = (filter: Parameters<typeof groupInventoryByLocation>[0]['filter']) =>
      groupInventoryByLocation({
        items,
        locations,
        filter,
        search: '',
        today: new Date('2026-04-27T12:00:00')
      })[0].productGroups.map((group) => group.bestBatch.id);

    expect(grouped('active')).toEqual(['expired', 'active', 'later']);
    expect(grouped('expiring_soon')).toEqual(['active']);
    expect(grouped('expired')).toEqual(['expired']);
    expect(grouped('depleted')).toEqual(['depleted']);
    expect(grouped('all')).toEqual(['expired', 'active', 'later', 'depleted']);
  });

  it('searches product brand location note unit and expiry fields', () => {
    const location = { id: 'pantry', name: 'Pantry' };
    const batch = {
      id: 'batch-1',
      product: { id: 'rice', name: 'Rice', brand: 'Acme' },
      location_id: 'pantry',
      location_name: 'Pantry',
      quantity: '2',
      unit: 'kg',
      expires_on: '2026-05-01',
      note: 'top shelf'
    };

    expect(matchesInventorySearch(batch, location, 'acme')).toBe(true);
    expect(matchesInventorySearch(batch, location, 'pantry')).toBe(true);
    expect(matchesInventorySearch(batch, location, 'top shelf')).toBe(true);
    expect(matchesInventorySearch(batch, location, 'kg')).toBe(true);
    expect(matchesInventorySearch(batch, location, '2026-05')).toBe(true);
    expect(matchesInventorySearch(batch, location, 'freezer')).toBe(false);
  });

  it('chooses highlighted then earliest active then depleted batches', () => {
    const depleted = {
      id: 'depleted',
      product: { id: 'rice', name: 'Rice' },
      location_id: 'pantry',
      quantity: '0',
      unit: 'kg',
      depleted_at: '2026-04-01T00:00:00Z'
    };
    const later = {
      id: 'later',
      product: { id: 'rice', name: 'Rice' },
      location_id: 'pantry',
      quantity: '1',
      unit: 'kg',
      expires_on: '2026-05-03'
    };
    const sooner = {
      id: 'sooner',
      product: { id: 'rice', name: 'Rice' },
      location_id: 'pantry',
      quantity: '1',
      unit: 'kg',
      expires_on: '2026-05-01'
    };

    expect(chooseBestBatch([depleted, later, sooner])?.id).toBe('sooner');
    expect(chooseBestBatch([depleted, later, sooner], 'later')?.id).toBe('later');
    expect(chooseBestBatch([depleted])?.id).toBe('depleted');
  });

  it('normalizes product display helpers and unit choices', () => {
    const units = [
      { code: 'lb', family: 'mass' as const },
      { code: 'oz', family: 'mass' as const },
      { code: 'cup', family: 'volume' as const }
    ];

    expect(unitChoicesForFamily('mass')).toEqual(['g', 'kg']);
    expect(unitChoicesForFamily('mass', units)).toEqual(['lb', 'oz']);
    expect(unitChoicesForFamily('count', units)).toEqual(['piece']);
    expect(unitChoicesForFamily('volume')).toEqual(['ml', 'l']);
    expect(unitChoicesForFamily('count')).toEqual(['piece']);
    expect(productPreferredUnit({ id: 'product-1', name: 'Rice', family: 'mass' }, units)).toBe(
      'lb'
    );
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

  it('hydrates batch edit fields from generated and compatibility field names', () => {
    expect(
      stockEditFields({
        id: 'batch-1',
        product: { name: 'Rice' },
        quantity: '2',
        locationId: 'pantry',
        expiresOn: '2026-05-01',
        openedOn: '2026-04-01',
        note: 'top shelf'
      })
    ).toEqual({
      quantity: '2',
      locationId: 'pantry',
      expiresOn: '2026-05-01',
      openedOn: '2026-04-01',
      note: 'top shelf'
    });
  });

  it('validates stock edit quantity and location before submit', () => {
    const valid = {
      quantity: '1.5',
      locationId: 'pantry',
      expiresOn: '',
      openedOn: '',
      note: ''
    };

    expect(validateStockEditInput(valid)).toBeNull();
    expect(validateStockEditInput({ ...valid, quantity: '' })).toBe(
      'Enter a positive stock quantity.'
    );
    expect(validateStockEditInput({ ...valid, quantity: 'nope' })).toBe(
      'Enter a positive stock quantity.'
    );
    expect(validateStockEditInput({ ...valid, quantity: '-1' })).toBe(
      'Enter a positive stock quantity.'
    );
    expect(validateStockEditInput({ ...valid, quantity: '0' })).toBe(
      'Enter a positive stock quantity.'
    );
    expect(validateStockEditInput({ ...valid, locationId: '' })).toBe(
      'Choose a location before saving.'
    );
  });

  it('builds minimal stock update requests and explicit clears', () => {
    const batch = {
      id: 'batch-1',
      product: { name: 'Rice' },
      quantity: '2',
      location_id: 'pantry',
      expires_on: '2026-05-01',
      opened_on: '2026-04-01',
      note: 'top shelf'
    };

    expect(
      buildStockUpdateRequest(batch, {
        quantity: '1.5',
        locationId: 'freezer',
        expiresOn: '2026-06-01',
        openedOn: '',
        note: ''
      })
    ).toEqual([
      { op: 'replace', path: '/quantity', value: '1.5' },
      { op: 'replace', path: '/location_id', value: 'freezer' },
      { op: 'replace', path: '/expires_on', value: '2026-06-01' },
      { op: 'remove', path: '/opened_on' },
      { op: 'remove', path: '/note' }
    ]);

    expect(buildStockUpdateRequest(batch, stockEditFields(batch))).toEqual([]);
    expect(
      buildStockUpdateRequest(
        { id: 'batch-2', product: { name: 'Beans' }, quantity: '3', location_id: 'pantry' },
        {
          quantity: '3',
          locationId: 'pantry',
          expiresOn: '',
          openedOn: '',
          note: ''
        }
      )
    ).toEqual([]);
  });
});
