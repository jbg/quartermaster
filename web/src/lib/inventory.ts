import type {
  Location,
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

export type InventoryFilterMode = 'active' | 'expiring_soon' | 'expired' | 'depleted' | 'all';

export interface InventoryProductGroup {
  locationId: string;
  productId: string;
  productName: string;
  productBrand: string;
  batches: StockBatch[];
  visibleBatches: StockBatch[];
  activeCount: number;
  depletedCount: number;
  totalQuantity: string | null;
  totalUnit: string | null;
  earliestExpiry: string | null;
  bestBatch: StockBatch;
}

export interface InventoryLocationGroup {
  location: Location;
  activeCount: number;
  depletedCount: number;
  productGroups: InventoryProductGroup[];
  emptyMessage: string;
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

export function groupInventoryByLocation(input: {
  items: StockBatch[];
  locations: Location[];
  filter: InventoryFilterMode;
  search: string;
  today?: Date;
  highlightBatchId?: string | null;
}): InventoryLocationGroup[] {
  const today = localDateKey(input.today ?? new Date());
  const search = normalizeSearch(input.search);
  return input.locations.map((location) => {
    const locationBatches = input.items.filter((item) => stockLocationId(item) === location.id);
    const visible = locationBatches.filter(
      (batch) =>
        matchesInventoryFilter(batch, input.filter, today) &&
        matchesInventorySearch(batch, location, search)
    );
    return {
      location,
      activeCount: locationBatches.filter((item) => !isDepleted(item)).length,
      depletedCount: locationBatches.filter(isDepleted).length,
      productGroups: groupVisibleBatches(location.id, visible, input.highlightBatchId ?? null),
      emptyMessage: inventoryLocationEmptyMessage(location.name, input.filter, search.length > 0)
    };
  });
}

export function matchesInventoryFilter(
  batch: StockBatch,
  filter: InventoryFilterMode,
  todayKey = localDateKey(new Date())
): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'depleted':
      return isDepleted(batch);
    case 'active':
      return !isDepleted(batch);
    case 'expired': {
      const expiry = stockExpiryValue(batch);
      return !isDepleted(batch) && expiry !== null && expiry < todayKey;
    }
    case 'expiring_soon': {
      const expiry = stockExpiryValue(batch);
      if (isDepleted(batch) || expiry === null) {
        return false;
      }
      const days = daysBetweenDateKeys(todayKey, expiry);
      return days >= 0 && days < 7;
    }
  }
}

export function matchesInventorySearch(
  batch: StockBatch,
  location: Location,
  normalizedQuery: string
): boolean {
  if (!normalizedQuery) {
    return true;
  }
  return [
    stockName(batch),
    batchProductBrand(batch),
    location.name,
    stockLocation(batch),
    batch.note ?? '',
    stockUnit(batch),
    stockExpiryValue(batch) ?? ''
  ]
    .join(' ')
    .toLowerCase()
    .includes(normalizedQuery);
}

export function chooseBestBatch(
  batches: StockBatch[],
  highlightBatchId: string | null = null
): StockBatch {
  const highlighted = highlightBatchId
    ? batches.find((batch) => batch.id === highlightBatchId)
    : undefined;
  if (highlighted) {
    return highlighted;
  }
  return [...batches].sort(compareBatchesForSelection)[0];
}

export function inventoryLocationEmptyMessage(
  locationName: string,
  filter: InventoryFilterMode,
  hasSearch: boolean
): string {
  if (hasSearch) {
    return 'No matching stock in this location.';
  }
  switch (filter) {
    case 'active':
      return `Nothing active in ${locationName}.`;
    case 'expiring_soon':
      return `Nothing expiring soon in ${locationName}.`;
    case 'expired':
      return `Nothing expired in ${locationName}.`;
    case 'depleted':
      return `No depleted history in ${locationName}.`;
    case 'all':
      return `Nothing in ${locationName} yet.`;
  }
}

export function batchProductId(batch: StockBatch): string {
  return batch.product?.id ?? '';
}

export function stockName(batch: StockBatch): string {
  return batch.product?.name ?? batch.product_name ?? batch.productName ?? 'Unnamed product';
}

export function batchProductBrand(batch: StockBatch): string {
  return batch.product?.brand?.trim() ?? '';
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

export function inventoryFilterLabel(filter: InventoryFilterMode): string {
  switch (filter) {
    case 'active':
      return 'Active';
    case 'expiring_soon':
      return 'Expiring soon';
    case 'expired':
      return 'Expired';
    case 'depleted':
      return 'Depleted';
    case 'all':
      return 'All';
  }
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

function groupVisibleBatches(
  locationId: string,
  visibleBatches: StockBatch[],
  highlightBatchId: string | null
): InventoryProductGroup[] {
  const byProduct = new Map<string, StockBatch[]>();
  for (const batch of visibleBatches) {
    const productId = batchProductId(batch) || stockName(batch);
    byProduct.set(productId, [...(byProduct.get(productId) ?? []), batch]);
  }

  return Array.from(byProduct.entries())
    .map(([productId, batches]) => {
      const bestBatch = chooseBestBatch(batches, highlightBatchId);
      const visibleSorted = [...batches].sort(compareBatchesForSelection);
      const active = batches.filter((batch) => !isDepleted(batch));
      const depleted = batches.filter(isDepleted);
      const unit = commonUnit(active.length > 0 ? active : batches);
      const totalQuantity = unit ? sumQuantities(active.length > 0 ? active : batches, unit) : null;
      return {
        locationId,
        productId,
        productName: stockName(bestBatch),
        productBrand: batchProductBrand(bestBatch),
        batches: visibleSorted,
        visibleBatches: visibleSorted,
        activeCount: active.length,
        depletedCount: depleted.length,
        totalQuantity,
        totalUnit: unit,
        earliestExpiry: earliestExpiry(active.length > 0 ? active : batches),
        bestBatch
      };
    })
    .sort(compareProductGroups);
}

function compareProductGroups(left: InventoryProductGroup, right: InventoryProductGroup): number {
  const leftDepletedOnly = left.activeCount === 0;
  const rightDepletedOnly = right.activeCount === 0;
  if (leftDepletedOnly !== rightDepletedOnly) {
    return leftDepletedOnly ? 1 : -1;
  }
  if (left.earliestExpiry && right.earliestExpiry && left.earliestExpiry !== right.earliestExpiry) {
    return left.earliestExpiry.localeCompare(right.earliestExpiry);
  }
  if (left.earliestExpiry !== right.earliestExpiry) {
    return left.earliestExpiry ? -1 : 1;
  }
  return left.productName.localeCompare(right.productName);
}

function compareBatchesForSelection(left: StockBatch, right: StockBatch): number {
  const leftDepleted = isDepleted(left);
  const rightDepleted = isDepleted(right);
  if (leftDepleted !== rightDepleted) {
    return leftDepleted ? 1 : -1;
  }
  const leftExpiry = stockExpiryValue(left);
  const rightExpiry = stockExpiryValue(right);
  if (leftExpiry && rightExpiry && leftExpiry !== rightExpiry) {
    return leftExpiry.localeCompare(rightExpiry);
  }
  if (leftExpiry !== rightExpiry) {
    return leftExpiry ? -1 : 1;
  }
  return stockCreated(left).localeCompare(stockCreated(right));
}

function commonUnit(batches: StockBatch[]): string | null {
  if (batches.length === 0) {
    return null;
  }
  const unit = stockUnit(batches[0]);
  return unit && batches.every((batch) => stockUnit(batch) === unit) ? unit : null;
}

function sumQuantities(batches: StockBatch[], unit: string): string | null {
  const values = batches.map((batch) => Number(batch.quantity));
  if (values.some((value) => !Number.isFinite(value))) {
    return null;
  }
  const total = values.reduce((sum, value) => sum + value, 0);
  return Number.isInteger(total) ? String(total) : String(Number(total.toFixed(3)));
}

function earliestExpiry(batches: StockBatch[]): string | null {
  return (
    batches
      .map(stockExpiryValue)
      .filter((value): value is string => value !== null)
      .sort()[0] ?? null
  );
}

function normalizeSearch(value: string): string {
  return value.trim().toLowerCase();
}

function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function daysBetweenDateKeys(start: string, end: string): number {
  const startDate = new Date(`${start}T00:00:00`);
  const endDate = new Date(`${end}T00:00:00`);
  return Math.round((endDate.getTime() - startDate.getTime()) / 86_400_000);
}
