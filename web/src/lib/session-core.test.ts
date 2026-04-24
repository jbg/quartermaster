import { describe, expect, it } from 'vitest';
import {
  QuartermasterSession,
  defaultServerUrl,
  type SessionStorage,
  type SessionTransport,
  type StoredSession
} from './session-core';

function memoryStorage(initial: StoredSession): SessionStorage & { value: StoredSession } {
  return {
    value: { ...initial },
    read() {
      return { ...this.value };
    },
    write(session) {
      this.value = { ...session };
    },
    clear() {
      this.value = { ...this.value, accessToken: null, refreshToken: null };
    }
  };
}

describe('QuartermasterSession', () => {
  it('uses the current origin as the default server URL', () => {
    expect(defaultServerUrl({ origin: 'http://localhost:8080', pathname: '/' })).toBe(
      'http://localhost:8080'
    );
  });

  it('preserves a Home Assistant ingress path in the default server URL', () => {
    expect(
      defaultServerUrl({
        origin: 'http://homeassistant.local:8123',
        pathname: '/api/hassio_ingress/quartermaster-token/'
      })
    ).toBe('http://homeassistant.local:8123/api/hassio_ingress/quartermaster-token');
  });

  it('drops SPA route segments from the ingress default server URL', () => {
    expect(
      defaultServerUrl({
        origin: 'http://homeassistant.local:8123',
        pathname: '/api/hassio_ingress/quartermaster-token/join'
      })
    ).toBe('http://homeassistant.local:8123/api/hassio_ingress/quartermaster-token');
  });

  it('refreshes once and retries an authenticated request after a 401', async () => {
    const storage = memoryStorage({
      serverUrl: 'http://localhost:8080',
      accessToken: 'old-access',
      refreshToken: 'old-refresh'
    });
    const calls: string[] = [];
    const transport: SessionTransport = {
      configure(session) {
        calls.push(`configure:${session.accessToken ?? 'none'}`);
      },
      async login() {
        throw new Error('unused');
      },
      async register() {
        throw new Error('unused');
      },
      async refresh() {
        calls.push('refresh');
        return {
          data: { access_token: 'new-access', refresh_token: 'new-refresh' },
          response: { status: 200 }
        };
      },
      async logout() {
        return { response: { status: 204 } };
      },
      async me() {
        calls.push('me');
        if (calls.filter((call) => call === 'me').length === 1) {
          return { error: {}, response: { status: 401 } };
        }
        return {
          data: { current_household: { id: 'home', name: 'Home' }, households: [] },
          response: { status: 200 }
        };
      },
      async switchHousehold() {
        throw new Error('unused');
      },
      async locationsList() {
        throw new Error('unused');
      },
      async productSearch() {
        throw new Error('unused');
      },
      async productCreate() {
        throw new Error('unused');
      },
      async stockList() {
        throw new Error('unused');
      },
      async stockCreate() {
        throw new Error('unused');
      },
      async stockGet() {
        throw new Error('unused');
      },
      async stockUpdate() {
        throw new Error('unused');
      },
      async stockListBatchEvents() {
        throw new Error('unused');
      },
      async stockConsume() {
        throw new Error('unused');
      },
      async stockDelete() {
        throw new Error('unused');
      },
      async stockRestore() {
        throw new Error('unused');
      },
      async remindersList() {
        throw new Error('unused');
      },
      async remindersPresent() {
        throw new Error('unused');
      },
      async remindersOpen() {
        throw new Error('unused');
      },
      async remindersAck() {
        throw new Error('unused');
      }
    };

    const session = new QuartermasterSession(storage, transport);
    const me = await session.me();

    expect(me.current_household?.name).toBe('Home');
    expect(storage.value.accessToken).toBe('new-access');
    expect(calls).toEqual(['configure:old-access', 'me', 'refresh', 'configure:new-access', 'me']);
  });

  it('clears tokens when refresh fails', async () => {
    const storage = memoryStorage({
      serverUrl: 'http://localhost:8080',
      accessToken: 'old-access',
      refreshToken: 'old-refresh'
    });
    const transport: SessionTransport = {
      configure() {},
      async login() {
        throw new Error('unused');
      },
      async register() {
        throw new Error('unused');
      },
      async refresh() {
        return { error: {}, response: { status: 401 } };
      },
      async logout() {
        return { response: { status: 204 } };
      },
      async me() {
        return { error: {}, response: { status: 401 } };
      },
      async switchHousehold() {
        throw new Error('unused');
      },
      async locationsList() {
        throw new Error('unused');
      },
      async productSearch() {
        throw new Error('unused');
      },
      async productCreate() {
        throw new Error('unused');
      },
      async stockList() {
        throw new Error('unused');
      },
      async stockCreate() {
        throw new Error('unused');
      },
      async stockGet() {
        throw new Error('unused');
      },
      async stockUpdate() {
        throw new Error('unused');
      },
      async stockListBatchEvents() {
        throw new Error('unused');
      },
      async stockConsume() {
        throw new Error('unused');
      },
      async stockDelete() {
        throw new Error('unused');
      },
      async stockRestore() {
        throw new Error('unused');
      },
      async remindersList() {
        throw new Error('unused');
      },
      async remindersPresent() {
        throw new Error('unused');
      },
      async remindersOpen() {
        throw new Error('unused');
      },
      async remindersAck() {
        throw new Error('unused');
      }
    };

    const session = new QuartermasterSession(storage, transport);

    await expect(session.me()).rejects.toMatchObject({ status: 401 });
    expect(storage.value.accessToken).toBeNull();
    expect(storage.value.refreshToken).toBeNull();
  });

  it('passes product and stock creation calls through the authenticated transport', async () => {
    const storage = memoryStorage({
      serverUrl: 'http://localhost:8080',
      accessToken: 'access',
      refreshToken: 'refresh'
    });
    const calls: string[] = [];
    const transport: SessionTransport = {
      configure() {},
      async login() {
        throw new Error('unused');
      },
      async register() {
        throw new Error('unused');
      },
      async refresh() {
        throw new Error('unused');
      },
      async logout() {
        return { response: { status: 204 } };
      },
      async me() {
        throw new Error('unused');
      },
      async switchHousehold() {
        throw new Error('unused');
      },
      async locationsList() {
        throw new Error('unused');
      },
      async productSearch(query) {
        calls.push(`search:${query.q}:${query.limit}`);
        return {
          data: { items: [{ id: 'product-1', name: 'Rice', family: 'mass', preferred_unit: 'g' }] },
          response: { status: 200 }
        };
      },
      async productCreate(body) {
        calls.push(`product:${body.name}:${body.family}`);
        return {
          data: { id: 'product-2', name: body.name, family: body.family, preferred_unit: 'kg' },
          response: { status: 201 }
        };
      },
      async stockList() {
        throw new Error('unused');
      },
      async stockCreate(body) {
        calls.push(`stock:${body.product_id}:${body.quantity}:${body.unit}`);
        return {
          data: {
            id: 'batch-1',
            product: { id: body.product_id, name: 'Rice', unit_family: 'mass' },
            quantity: body.quantity,
            unit: body.unit
          },
          response: { status: 201 }
        };
      },
      async stockGet() {
        throw new Error('unused');
      },
      async stockUpdate(id, body) {
        calls.push(`update:${id}:${body.quantity}:${body.expires_on}`);
        return {
          data: {
            id,
            product: { id: 'product-2', name: 'Rice', unit_family: 'mass' },
            quantity: body.quantity ?? '2',
            unit: 'kg',
            expires_on: body.expires_on
          },
          response: { status: 200 }
        };
      },
      async stockListBatchEvents() {
        throw new Error('unused');
      },
      async stockConsume() {
        throw new Error('unused');
      },
      async stockDelete() {
        throw new Error('unused');
      },
      async stockRestore() {
        throw new Error('unused');
      },
      async remindersList() {
        throw new Error('unused');
      },
      async remindersPresent() {
        throw new Error('unused');
      },
      async remindersOpen() {
        throw new Error('unused');
      },
      async remindersAck() {
        throw new Error('unused');
      }
    };

    const session = new QuartermasterSession(storage, transport);

    await expect(session.productSearch({ q: 'rice', limit: 12 })).resolves.toMatchObject({
      items: [{ name: 'Rice' }]
    });
    await expect(
      session.productCreate({ name: 'Manual Rice', family: 'mass', preferred_unit: 'kg' })
    ).resolves.toMatchObject({ name: 'Manual Rice' });
    await expect(
      session.stockCreate({
        product_id: 'product-2',
        location_id: 'pantry',
        quantity: '2',
        unit: 'kg'
      })
    ).resolves.toMatchObject({ id: 'batch-1' });
    await expect(
      session.stockUpdate('batch-1', { quantity: '1.5', expires_on: '2026-05-01' })
    ).resolves.toMatchObject({ id: 'batch-1', quantity: '1.5' });
    expect(calls).toEqual([
      'search:rice:12',
      'product:Manual Rice:mass',
      'stock:product-2:2:kg',
      'update:batch-1:1.5:2026-05-01'
    ]);
  });
});
