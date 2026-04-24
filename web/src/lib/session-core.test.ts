import { describe, expect, it } from 'vitest';
import { QuartermasterSession, type SessionStorage, type SessionTransport, type StoredSession } from './session-core';

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
      async stockList() {
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
      async stockList() {
        throw new Error('unused');
      }
    };

    const session = new QuartermasterSession(storage, transport);

    await expect(session.me()).rejects.toMatchObject({ status: 401 });
    expect(storage.value.accessToken).toBeNull();
    expect(storage.value.refreshToken).toBeNull();
  });
});
