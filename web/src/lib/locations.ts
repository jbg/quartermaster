import { ApiFailure, type CreateLocationRequest, type Location } from './session-core';

export type LocationKind = 'pantry' | 'fridge' | 'freezer';

export interface LocationFormFields {
  name: string;
  kind: LocationKind;
}

export const locationKinds: LocationKind[] = ['pantry', 'fridge', 'freezer'];

export function locationSortOrder(location: Location): number {
  return location.sort_order ?? location.sortOrder ?? 0;
}

export function sortLocations(locations: Location[]): Location[] {
  return [...locations].sort(
    (a, b) =>
      locationSortOrder(a) - locationSortOrder(b) ||
      a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
  );
}

export function validateLocationName(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) {
    return 'Enter a location name.';
  }
  if (trimmed.length > 64) {
    return 'Location name must be 64 characters or fewer.';
  }
  return null;
}

export function isLocationKind(value: string): value is LocationKind {
  return locationKinds.includes(value as LocationKind);
}

export function normalizeLocationKind(value: string): LocationKind {
  return isLocationKind(value) ? value : 'pantry';
}

export function buildCreateLocationRequest(fields: LocationFormFields): CreateLocationRequest {
  return {
    name: fields.name.trim(),
    kind: fields.kind
  };
}

export function buildUpdateLocationRequest(location: Location, fields: LocationFormFields) {
  return {
    name: fields.name.trim(),
    kind: fields.kind,
    sort_order: locationSortOrder(location)
  };
}

export function locationDeleteErrorMessage(error: unknown): string {
  if (
    error instanceof ApiFailure &&
    (error.status === 409 || error.code === 'location_has_stock')
  ) {
    return 'This location still has active stock. Move, consume, or discard it first.';
  }
  return 'Location could not be deleted.';
}
