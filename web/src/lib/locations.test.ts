import { describe, expect, it } from 'vitest';
import { ApiFailure, type Location } from './session-core';
import {
  buildCreateLocationRequest,
  buildUpdateLocationRequest,
  locationDeleteErrorMessage,
  locationSortOrder,
  normalizeLocationKind,
  sortLocations,
  validateLocationName
} from './locations';

describe('location helpers', () => {
  it('validates trimmed location names', () => {
    expect(validateLocationName(' Pantry ')).toBeNull();
    expect(validateLocationName('   ')).toBe('Enter a location name.');
    expect(validateLocationName('x'.repeat(65))).toBe(
      'Location name must be 64 characters or fewer.'
    );
  });

  it('sorts by sort_order then name', () => {
    const locations: Location[] = [
      { id: 'freezer', name: 'Freezer', kind: 'freezer', sort_order: 2 },
      { id: 'cellar', name: 'Cellar', kind: 'pantry', sort_order: 1 },
      { id: 'fridge', name: 'Fridge', kind: 'fridge', sort_order: 1 }
    ];

    expect(sortLocations(locations).map((location) => location.id)).toEqual([
      'cellar',
      'fridge',
      'freezer'
    ]);
    expect(locationSortOrder({ id: 'legacy', name: 'Legacy', sortOrder: 7 })).toBe(7);
  });

  it('normalizes kind and builds API payloads', () => {
    expect(normalizeLocationKind('fridge')).toBe('fridge');
    expect(normalizeLocationKind('garage')).toBe('pantry');
    expect(buildCreateLocationRequest({ name: '  Shelf  ', kind: 'pantry' })).toEqual({
      name: 'Shelf',
      kind: 'pantry'
    });
    expect(
      buildUpdateLocationRequest(
        { id: 'loc-1', name: 'Old', kind: 'freezer', sort_order: 3 },
        { name: '  New  ', kind: 'fridge' }
      )
    ).toEqual({
      name: 'New',
      kind: 'fridge',
      sort_order: 3
    });
  });

  it('maps active-stock delete conflicts to a user-facing message', () => {
    expect(locationDeleteErrorMessage(new ApiFailure(409, 'conflict', 'location_has_stock'))).toBe(
      'This location still has active stock. Move, consume, or discard it first.'
    );
    expect(locationDeleteErrorMessage(new Error('nope'))).toBe('Location could not be deleted.');
  });
});
