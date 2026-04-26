import {
  authLogin,
  authLogout,
  authMe,
  authRefresh,
  authRegister,
  authSwitchHousehold,
  locationsCreate,
  locationsDelete,
  locationsList,
  locationsUpdate,
  productByBarcode,
  productCreate,
  productSearch,
  productDelete,
  productGet,
  productList,
  productRefresh,
  productRestore,
  productUpdate,
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
  stockUpdate,
  unitsList
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
    locationsCreate(body) {
      return locationsCreate({ body });
    },
    locationsUpdate(id, body) {
      return locationsUpdate({ path: { id }, body });
    },
    locationsDelete(id) {
      return locationsDelete({ path: { id } });
    },
    productSearch(query) {
      return productSearch({ query });
    },
    productList(query) {
      return productList(query ? { query } : undefined);
    },
    productByBarcode(barcode) {
      return productByBarcode({ path: { barcode } });
    },
    productCreate(body) {
      return productCreate({ body });
    },
    productGet(id) {
      return productGet({ path: { id } });
    },
    productUpdate(id, body) {
      return productUpdate({ path: { id }, body });
    },
    productDelete(id) {
      return productDelete({ path: { id } });
    },
    productRestore(id) {
      return productRestore({ path: { id } });
    },
    productRefresh(id) {
      return productRefresh({ path: { id } });
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
    unitsList() {
      return unitsList();
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
