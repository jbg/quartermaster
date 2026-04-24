import {
  authLogin,
  authLogout,
  authMe,
  authRefresh,
  authRegister,
  authSwitchHousehold,
  stockList
} from './generated/sdk.gen';
import { client } from './generated/client.gen';
import type { SessionTransport, StoredSession } from './session-core';

function normalizedBaseUrl(serverUrl: string): string {
  return serverUrl.replace(/\/+$/, '');
}

export function generatedTransport(): SessionTransport {
  let current: StoredSession = {
    serverUrl: '',
    accessToken: null,
    refreshToken: null
  };

  return {
    configure(session) {
      current = { ...session };
      client.setConfig({
        baseUrl: normalizedBaseUrl(current.serverUrl),
        auth: () => current.accessToken ?? undefined
      });
    },
    login(body) {
      return authLogin({ body });
    },
    register(body) {
      return authRegister({ body });
    },
    refresh(body) {
      return authRefresh({ body });
    },
    logout() {
      return authLogout();
    },
    me() {
      return authMe();
    },
    switchHousehold(body) {
      return authSwitchHousehold({ body });
    },
    stockList() {
      return stockList();
    }
  };
}
