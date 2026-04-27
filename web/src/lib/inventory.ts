import type {
  Product,
  QuartermasterSession,
  StockBatch,
  StockEvent,
  Unit,
  UnitFamily,
  UpdateStockRequest
} from './session-core';

export interface InventoryState {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  items: StockBatch[];
  error: string | null;
}

export interface InventoryGroups {
  active: StockBatch[];
  depleted: StockBatch[];
}

export interface StockEditFields {
  quantity: string;
  locationId: string;
  expiresOn: string;
  openedOn: string;
  note: string;
}

export const emptyInventoryState: InventoryState = {
  status: 'idle',
  items: [],
  error: null
};

export const fallbackUnitChoicesByFamily: Record<UnitFamily, string[]> = {
  mass: ['g', 'kg'],
  volume: ['ml', 'l'],
  count: ['piece']
};

export async function loadInventory(
  session: Pick<QuartermasterSession, 'stockList'>
): Promise<InventoryState> {
  try {
    const response = await session.stockList({ include_depleted: true });
    return {
      status: 'loaded',
      items: response.items ?? [],
      error: null
    };
  } catch {
    return {
      status: 'error',
      items: [],
      error: 'Inventory could not be loaded.'
    };
  }
}

export function groupInventory(items: StockBatch[]): InventoryGroups {
  return {
    active: items.filter((item) => !isDepleted(item)),
    depleted: items.filter(isDepleted)
  };
}

export function batchProductId(batch: StockBatch): string {
  return batch.product?.id ?? '';
}

export function stockName(batch: StockBatch): string {
  return batch.product?.name ?? batch.product_name ?? batch.productName ?? 'Unnamed product';
}

export function stockUnit(batch: StockBatch): string {
  return (
    (typeof batch.unit === 'string' ? batch.unit : batch.unit?.code) ??
    batch.unit_code ??
    batch.unitCode ??
    ''
  );
}

export function stockLocation(batch: StockBatch): string {
  return batch.location?.name ?? batch.location_name ?? batch.locationName ?? 'No location';
}

export function stockLocationId(batch: StockBatch): string | null {
  return batch.location_id ?? batch.locationId ?? null;
}

export function stockExpiry(batch: StockBatch): string {
  return stockExpiryValue(batch) ?? 'No expiry date';
}

export function stockOpened(batch: StockBatch): string {
  return stockOpenedValue(batch) ?? 'Not marked opened';
}

export function stockExpiryValue(batch: StockBatch): string | null {
  return batch.expires_on ?? batch.expiresOn ?? null;
}

export function stockOpenedValue(batch: StockBatch): string | null {
  return batch.opened_on ?? batch.openedOn ?? null;
}

export function stockCreated(batch: StockBatch): string {
  return batch.created_at ?? batch.createdAt ?? '';
}

export function stockInitialQuantity(batch: StockBatch): string {
  const value = batch.initial_quantity ?? batch.initialQuantity;
  return value === undefined || value === null ? '' : String(value);
}

export function isDepleted(batch: StockBatch): boolean {
  return Boolean(batch.depleted_at ?? batch.depletedAt);
}

export function stockDepletedAt(batch: StockBatch): string {
  return batch.depleted_at ?? batch.depletedAt ?? '';
}

export function eventType(event: StockEvent): StockEvent['event_type'] {
  return event.event_type ?? event.eventType;
}

export function eventDelta(event: StockEvent): string {
  return event.quantity_delta ?? event.quantityDelta ?? '';
}

export function eventCreated(event: StockEvent): string {
  return event.created_at ?? event.createdAt ?? '';
}

export function eventActor(event: StockEvent): string {
  return event.created_by_username ?? event.createdByUsername ?? 'Unknown user';
}

export function eventBatchId(event: StockEvent): string {
  return event.batch_id ?? event.batchId ?? '';
}

export function canRestoreBatch(batch: StockBatch | null, events: StockEvent[]): boolean {
  if (!batch || !isDepleted(batch) || events.length === 0) {
    return false;
  }
  return eventType(events[0]) === 'discard';
}

export function selectBatchAfterRefresh(
  items: StockBatch[],
  preferredId: string | null
): StockBatch | null {
  if (preferredId) {
    const preferred = items.find((item) => item.id === preferredId);
    if (preferred) {
      return preferred;
    }
  }
  return items[0] ?? null;
}

export function productBrand(product: Product): string {
  return product.brand?.trim() ?? '';
}

export function productPreferredUnit(product: Product, units: Unit[] = []): string {
  return (
    product.preferred_unit ??
    product.preferredUnit ??
    unitChoicesForFamily(product.family, units)[0]
  );
}

export function productSource(product: Product): string {
  return product.source === 'openfoodfacts' ? 'OpenFoodFacts' : 'Manual';
}

export function unitChoicesForFamily(family: UnitFamily, units: Unit[] = []): string[] {
  const choices = units
    .filter((unit) => unit.family === family)
    .map((unit) => unit.code)
    .sort();
  return choices.length > 0 ? choices : fallbackUnitChoicesByFamily[family];
}

export function validateAddStockInput(input: {
  product: Product | null;
  quantity: string;
  locationId: string;
}): string | null {
  if (!input.product) {
    return 'Select or create a product first.';
  }
  if (!input.locationId) {
    return 'Choose a location before adding stock.';
  }
  const quantity = Number(input.quantity.trim());
  if (!input.quantity.trim() || !Number.isFinite(quantity) || quantity <= 0) {
    return 'Enter a positive stock quantity.';
  }
  return null;
}

export function stockEditFields(batch: StockBatch): StockEditFields {
  return {
    quantity: batch.quantity === undefined || batch.quantity === null ? '' : String(batch.quantity),
    locationId: stockLocationId(batch) ?? '',
    expiresOn: stockExpiryValue(batch) ?? '',
    openedOn: stockOpenedValue(batch) ?? '',
    note: batch.note ?? ''
  };
}

export function validateStockEditInput(input: StockEditFields): string | null {
  const quantity = Number(input.quantity.trim());
  if (!input.quantity.trim() || !Number.isFinite(quantity) || quantity <= 0) {
    return 'Enter a positive stock quantity.';
  }
  if (!input.locationId) {
    return 'Choose a location before saving.';
  }
  return null;
}

export function buildStockUpdateRequest(
  batch: StockBatch,
  input: StockEditFields
): UpdateStockRequest {
  const request: UpdateStockRequest = [];
  const quantity = input.quantity.trim();
  const currentQuantity =
    batch.quantity === undefined || batch.quantity === null ? '' : String(batch.quantity);
  if (quantity !== currentQuantity) {
    request.push({ op: 'replace', path: '/quantity', value: quantity });
  }

  const currentLocationId = stockLocationId(batch) ?? '';
  if (input.locationId && input.locationId !== currentLocationId) {
    request.push({ op: 'replace', path: '/location_id', value: input.locationId });
  }

  applyOptionalDate(request, 'expires_on', stockExpiryValue(batch), input.expiresOn);
  applyOptionalDate(request, 'opened_on', stockOpenedValue(batch), input.openedOn);

  const currentNote = batch.note ?? null;
  const nextNote = input.note.trim();
  if (nextNote) {
    if (nextNote !== currentNote) {
      request.push({ op: 'replace', path: '/note', value: nextNote });
    }
  } else if (currentNote !== null) {
    request.push({ op: 'remove', path: '/note' });
  }

  return request;
}

function applyOptionalDate(
  request: UpdateStockRequest,
  key: 'expires_on' | 'opened_on',
  currentValue: string | null,
  nextValue: string
) {
  const trimmed = nextValue.trim();
  if (trimmed) {
    if (trimmed !== currentValue) {
      request.push({ op: 'replace', path: `/${key}`, value: trimmed });
    }
  } else if (currentValue !== null) {
    request.push({ op: 'remove', path: `/${key}` });
  }
}
