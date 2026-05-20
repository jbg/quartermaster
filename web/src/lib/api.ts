import {
  accountOpenfoodfactsDelete,
  accountOpenfoodfactsPut,
  accountOpenfoodfactsStatus,
  authLogin,
  authLogout,
  authMe,
  authPasswordResetConfirm,
  authPasswordResetRequest,
  authRefresh,
  authRegister,
  authSwitchHousehold,
  deviceRegister,
  householdCurrentDeletionRequest,
  householdCurrentExport,
  householdCurrentGet,
  householdCurrentUpdate,
  householdImport,
  labelPrintersCreate,
  labelPrintersDelete,
  labelPrintersList,
  labelPrintersTest,
  labelPrintersTestRender,
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
  productOffContribution,
  productOffContributionPreview,
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
  stockLabelRender,
  stockList,
  stockListBatchEvents,
  stockRestore,
  stockSplit,
  stockUpdate,
  storageVesselsCreate,
  storageVesselsDelete,
  storageVesselsList,
  storageVesselsUpdate,
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
    passwordResetRequest(body) {
      return authPasswordResetRequest({ body });
    },
    passwordResetConfirm(body) {
      return authPasswordResetConfirm({ body });
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
    householdCurrentExport() {
      return householdCurrentExport();
    },
    householdImport(body) {
      return householdImport({ body });
    },
    householdCurrentDeletionRequest(body) {
      return householdCurrentDeletionRequest({ body });
    },
    householdCurrentGet() {
      return householdCurrentGet();
    },
    householdCurrentUpdate(body) {
      return householdCurrentUpdate({ body });
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
    storageVesselsList() {
      return storageVesselsList();
    },
    storageVesselsCreate(body) {
      return storageVesselsCreate({ body });
    },
    storageVesselsUpdate(id, body) {
      return storageVesselsUpdate({ path: { id }, body });
    },
    storageVesselsDelete(id) {
      return storageVesselsDelete({ path: { id } });
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
    labelPrintersTestRender(id) {
      return labelPrintersTestRender({ path: { id } });
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
    productOffContributionPreview(id) {
      return productOffContributionPreview({ path: { id } });
    },
    productOffContribution(id) {
      return productOffContribution({ path: { id } });
    },
    accountOpenfoodfactsStatus() {
      return accountOpenfoodfactsStatus();
    },
    accountOpenfoodfactsPut(body) {
      return accountOpenfoodfactsPut({ body });
    },
    accountOpenfoodfactsDelete() {
      return accountOpenfoodfactsDelete();
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
    stockSplit(id, body) {
      return stockSplit({ path: { id }, body });
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
    stockLabelRender(id, body) {
      return stockLabelRender({ path: { id }, body });
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
