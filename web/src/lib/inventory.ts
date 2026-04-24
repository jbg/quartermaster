import type { QuartermasterSession, StockBatch } from './session-core';

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

export async function loadInventory(session: Pick<QuartermasterSession, 'stockList'>): Promise<InventoryState> {
  try {
    const response = await session.stockList();
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

export function stockName(batch: StockBatch): string {
  return batch.product?.name ?? batch.product_name ?? batch.productName ?? 'Unnamed product';
}

export function stockUnit(batch: StockBatch): string {
  return (typeof batch.unit === 'string' ? batch.unit : batch.unit?.code) ?? batch.unit_code ?? batch.unitCode ?? '';
}

export function stockLocation(batch: StockBatch): string {
  return batch.location?.name ?? batch.location_name ?? batch.locationName ?? 'No location';
}

export function stockExpiry(batch: StockBatch): string {
  return batch.expires_on ?? batch.expiresOn ?? 'No expiry date';
}

export function isDepleted(batch: StockBatch): boolean {
  return Boolean(batch.depleted_at ?? batch.depletedAt);
}
