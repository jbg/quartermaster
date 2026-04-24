import {
  authLogin,
  authLogout,
  authMe,
  authRefresh,
  authRegister,
  authSwitchHousehold,
  locationsList,
  remindersAck,
  remindersList,
  remindersOpen,
  remindersPresent,
  stockConsume,
  stockDelete,
  stockGet,
  stockList,
  stockListBatchEvents,
  stockRestore
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
    locationsList() {
      return locationsList();
    },
    stockList(query) {
      return stockList({ query });
    },
    stockGet(id) {
      return stockGet({ path: { id } });
    },
    stockListBatchEvents(id, query) {
      return stockListBatchEvents({ path: { id }, query });
    },
    stockConsume(body) {
      return stockConsume({ body });
    },
    stockDelete(id) {
      return stockDelete({ path: { id } });
    },
    stockRestore(id) {
      return stockRestore({ path: { id } });
    },
    remindersList(query) {
      return remindersList({ query });
    },
    remindersPresent(id) {
      return remindersPresent({ path: { id } });
    },
    remindersOpen(id) {
      return remindersOpen({ path: { id } });
    },
    remindersAck(id) {
      return remindersAck({ path: { id } });
    }
  };
}
