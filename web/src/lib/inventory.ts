import type {
  Product,
  QuartermasterSession,
  StockBatch,
  StockEvent,
  UnitFamily
} from './session-core';

export interface InventoryState {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  items: StockBatch[];
  error: string | null;
}

export const emptyInventoryState: InventoryState = {
  status: 'idle',
  items: [],
  error: null
};

export const unitChoicesByFamily: Record<UnitFamily, string[]> = {
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
  return batch.expires_on ?? batch.expiresOn ?? 'No expiry date';
}

export function stockOpened(batch: StockBatch): string {
  return batch.opened_on ?? batch.openedOn ?? 'Not marked opened';
}

export function stockCreated(batch: StockBatch): string {
  return batch.created_at ?? batch.createdAt ?? '';
}

export function stockInitialQuantity(batch: StockBatch): string {
  const value = batch.initial_quantity ?? batch.initialQuantity;
  return value === undefined || value === null ? '' : String(value);
}

export function isDepleted(batch: StockBatch): boolean {
  const quantity =
    batch.quantity === undefined || batch.quantity === null ? null : Number(batch.quantity);
  return Boolean(batch.depleted_at ?? batch.depletedAt) || quantity === 0;
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

export function productPreferredUnit(product: Product): string {
  return product.preferred_unit ?? product.preferredUnit ?? unitChoicesByFamily[product.family][0];
}

export function productSource(product: Product): string {
  return product.source === 'openfoodfacts' ? 'OpenFoodFacts' : 'Manual';
}

export function unitChoicesForFamily(family: UnitFamily): string[] {
  return unitChoicesByFamily[family];
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
