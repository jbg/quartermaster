import {
  authLogin,
  authLogout,
  authMe,
  authRefresh,
  authRegister,
  authSwitchHousehold,
  deviceRegister,
  labelPrintersCreate,
  labelPrintersDelete,
  labelPrintersList,
  labelPrintersTest,
  labelPrintersUpdate,
  locationsCreate,
  locationsDelete,
  locationsList,
  locationsUpdate,
  onboardingCreateHousehold as onboardingCreateHouseholdRequest,
  onboardingJoinInvite as onboardingJoinInviteRequest,
  onboardingStatus as onboardingStatusRequest,
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
  stockLabelPrint,
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
    serverUrl: ''
  };
  installCsrfInterceptor();

  return {
    configure(session) {
      current = { ...session };
      client.setConfig({
        baseUrl: normalizedBaseUrl(current.serverUrl),
        credentials: 'include'
      });
    },
    login(body) {
      return authLogin({ body });
    },
    register(body) {
      return authRegister({ body });
    },
    onboardingStatus() {
      return onboardingStatusRequest();
    },
    createOnboardingHousehold(body) {
      return onboardingCreateHouseholdRequest({ body });
    },
    joinOnboardingInvite(body) {
      return onboardingJoinInviteRequest({ body });
    },
    refresh(body) {
      return authRefresh({ body: body ?? {} });
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
    registerDevice(body) {
      return deviceRegister({ body });
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
    labelPrintersList() {
      return labelPrintersList();
    },
    labelPrintersCreate(body) {
      return labelPrintersCreate({ body });
    },
    labelPrintersUpdate(id, body) {
      return labelPrintersUpdate({ path: { id }, body });
    },
    labelPrintersDelete(id) {
      return labelPrintersDelete({ path: { id } });
    },
    labelPrintersTest(id) {
      return labelPrintersTest({ path: { id } });
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
    stockLabelPrint(id, body) {
      return stockLabelPrint({ path: { id }, body });
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

let csrfInterceptorInstalled = false;

function installCsrfInterceptor(): void {
  if (csrfInterceptorInstalled) {
    return;
  }
  client.interceptors.request.use((request) => {
    return withCsrfHeader(request);
  });
  csrfInterceptorInstalled = true;
}

function requiresCsrf(method: string): boolean {
  return !['GET', 'HEAD', 'OPTIONS'].includes(method.toUpperCase());
}

export function browserCookie(name: string): string | null {
  if (typeof document === 'undefined') {
    return null;
  }
  return (
    document.cookie
      .split(';')
      .map((part) => part.trim())
      .find((part) => part.startsWith(`${name}=`))
      ?.slice(name.length + 1) ?? null
  );
}

export function withCsrfHeader(request: Request): Request {
  if (!requiresCsrf(request.method)) {
    return request;
  }
  const csrf = browserCookie('qm_csrf');
  if (!csrf) {
    return request;
  }
  const headers = new Headers(request.headers);
  headers.set('X-QM-CSRF', csrf);
  return new Request(request, { headers });
}
