import {
  authLogin,
  authLogout,
  authMe,
  authRefresh,
  authRegister,
  authSwitchHousehold,
  locationsList,
  productCreate,
  productSearch,
  remindersAck,
  remindersList,
  remindersOpen,
  remindersPresent,
  stockConsume,
  stockCreate,
  stockDelete,
  stockGet,
  stockList,
  stockListBatchEvents,
  stockRestore,
  stockUpdate
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
    productSearch(query) {
      return productSearch({ query });
    },
    productCreate(body) {
      return productCreate({ body });
    },
    stockList(query) {
      return stockList({ query });
    },
    stockCreate(body) {
      return stockCreate({ body });
    },
    stockGet(id) {
      return stockGet({ path: { id } });
    },
    stockUpdate(id, body) {
      return stockUpdate({ path: { id }, body });
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
