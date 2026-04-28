import { trimTrailingSlashes, webBasePath, type BrowserLocationLike } from '$lib/paths';

export interface TokenPair {
  access_token?: string;
  refresh_token?: string;
  accessToken?: string;
  refreshToken?: string;
}

export interface HouseholdSummary {
  id: string;
  name: string;
}

export interface MeResponse {
  current_household?: HouseholdSummary | null;
  currentHousehold?: HouseholdSummary | null;
  households?: HouseholdSummary[];
  user?: {
    username?: string;
  };
}

export interface Location {
  id: string;
  name: string;
  kind?: string;
  sort_order?: number;
  sortOrder?: number;
}

export type UnitFamily = 'mass' | 'volume' | 'count';

export interface Unit {
  code: string;
  family: UnitFamily;
  to_base_milli?: number;
  toBaseMilli?: number;
}

export interface Product {
  id: string;
  name: string;
  brand?: string | null;
  family: UnitFamily;
  preferred_unit?: string;
  preferredUnit?: string;
  image_url?: string | null;
  imageUrl?: string | null;
  barcode?: string | null;
  source?: 'openfoodfacts' | 'manual';
  deleted_at?: string | null;
  deletedAt?: string | null;
}

export interface ProductSearchResponse {
  items?: Product[];
}

export interface BarcodeLookupResponse {
  product: Product;
  source?: string;
}

export interface StockBatch {
  id: string;
  product?: {
    id?: string;
    name?: string;
    brand?: string | null;
    unit_family?: string;
    unitFamily?: string;
  };
  product_name?: string;
  productName?: string;
  quantity?: string | number;
  unit?:
    | string
    | {
        code?: string;
        name?: string;
      };
  unit_code?: string;
  unitCode?: string;
  note?: string | null;
  created_at?: string;
  createdAt?: string;
  initial_quantity?: string | number;
  initialQuantity?: string | number;
  location?: {
    name?: string;
  } | null;
  location_id?: string;
  locationId?: string;
  location_name?: string | null;
  locationName?: string | null;
  expires_on?: string | null;
  expiresOn?: string | null;
  opened_on?: string | null;
  openedOn?: string | null;
  depleted_at?: string | null;
  depletedAt?: string | null;
}

export interface StockListResponse {
  items?: StockBatch[];
}

export interface StockEvent {
  id: string;
  event_type?: 'add' | 'consume' | 'adjust' | 'discard' | 'restore';
  eventType?: 'add' | 'consume' | 'adjust' | 'discard' | 'restore';
  quantity_delta?: string;
  quantityDelta?: string;
  unit?: string;
  batch_expires_on?: string | null;
  batchExpiresOn?: string | null;
  note?: string | null;
  created_at?: string;
  createdAt?: string;
  created_by_username?: string | null;
  createdByUsername?: string | null;
  batch_id?: string;
  batchId?: string;
  product?: {
    name?: string;
  };
  consume_request_id?: string | null;
  consumeRequestId?: string | null;
}

export interface StockEventListResponse {
  items?: StockEvent[];
  next_before?: string | null;
  nextBefore?: string | null;
  next_before_id?: string | null;
  nextBeforeId?: string | null;
}

export interface Reminder {
  id: string;
  kind: 'expiry';
  fire_at: string;
  household_timezone: string;
  household_fire_local_at: string;
  expires_on?: string | null;
  days_until_expiry?: number | null;
  urgency?: 'expired' | 'expires_today' | 'expires_tomorrow' | 'expires_future' | null;
  batch_id: string;
  product_id: string;
  location_id: string;
  product_name: string;
  location_name: string;
  quantity: string;
  unit: string;
  presented_on_device_at?: string | null;
  presentedOnDeviceAt?: string | null;
  opened_on_device_at?: string | null;
  openedOnDeviceAt?: string | null;
}

export interface ReminderListResponse {
  items?: Reminder[];
  next_after_fire_at?: string | null;
  nextAfterFireAt?: string | null;
  next_after_id?: string | null;
  nextAfterId?: string | null;
}

export interface ConsumeRequest {
  product_id: string;
  location_id?: string | null;
  quantity: string;
  unit: string;
}

export interface ConsumeResponse {
  consume_request_id?: string;
  consumeRequestId?: string;
}

export interface CreateProductRequest {
  name: string;
  brand?: string | null;
  family: UnitFamily;
  preferred_unit?: string | null;
  barcode?: string | null;
  image_url?: string | null;
}

export interface JsonPatchOperation {
  op: 'replace' | 'remove';
  path: string;
  value?: unknown;
}

export type UpdateProductRequest = JsonPatchOperation[];

export interface CreateStockRequest {
  product_id: string;
  location_id: string;
  quantity: string;
  unit: string;
  expires_on?: string | null;
  opened_on?: string | null;
  note?: string | null;
}

export interface CreateLocationRequest {
  name: string;
  kind: string;
  sort_order?: number | null;
}

export interface UpdateLocationRequest {
  name: string;
  kind: string;
  sort_order: number;
}

export type UpdateStockRequest = JsonPatchOperation[];

export interface ApiResult<T> {
  data?: T;
  error?: unknown;
  response?: {
    status: number;
  };
}

export interface SessionStorage {
  read(): StoredSession;
  write(session: StoredSession): void;
  clear(): void;
}

export interface StoredSession {
  serverUrl: string;
  browserDeviceId?: string | null;
}

export interface OnboardingAuthMethodDescriptor {
  method: 'password' | 'passkey';
  availability: 'enabled' | 'unavailable';
  unavailable_reason?: string | null;
}

export interface SessionTransport {
  configure(session: StoredSession): void;
  login(body: { username: string; password: string }): Promise<ApiResult<TokenPair>>;
  onboardingStatus(): Promise<
    ApiResult<{
      server_state: 'needs_initial_setup' | 'ready';
      household_signup: 'enabled' | 'disabled';
      invite_join: 'enabled' | 'disabled';
      auth_methods: OnboardingAuthMethodDescriptor[];
    }>
  >;
  createOnboardingHousehold(body: {
    username: string;
    password: string;
    household_name: string;
    timezone: string;
  }): Promise<ApiResult<TokenPair>>;
  joinOnboardingInvite(body: {
    username: string;
    password: string;
    invite_code: string;
  }): Promise<ApiResult<TokenPair>>;
  register(body: {
    username: string;
    password: string;
    email?: string | null;
    invite_code?: string | null;
  }): Promise<ApiResult<TokenPair>>;
  refresh(body?: { refresh_token?: string | null }): Promise<ApiResult<TokenPair>>;
  logout(): Promise<ApiResult<void>>;
  me(): Promise<ApiResult<MeResponse>>;
  switchHousehold(body: { household_id: string }): Promise<ApiResult<MeResponse>>;
  registerDevice?(body: {
    device_id: string;
    platform: string;
    push_token?: string | null;
    push_authorization: 'not_determined' | 'denied' | 'authorized' | 'provisional';
    app_version?: string | null;
  }): Promise<ApiResult<void>>;
  locationsList(): Promise<ApiResult<Location[]>>;
  locationsCreate(body: CreateLocationRequest): Promise<ApiResult<Location>>;
  locationsUpdate(id: string, body: UpdateLocationRequest): Promise<ApiResult<Location>>;
  locationsDelete(id: string): Promise<ApiResult<void>>;
  productSearch(query: {
    q: string;
    limit?: number | null;
    include_deleted?: boolean | null;
  }): Promise<ApiResult<ProductSearchResponse>>;
  productList(query?: {
    q?: string | null;
    limit?: number | null;
    include_deleted?: boolean | null;
  }): Promise<ApiResult<ProductSearchResponse>>;
  productByBarcode(barcode: string): Promise<ApiResult<BarcodeLookupResponse>>;
  productCreate(body: CreateProductRequest): Promise<ApiResult<Product>>;
  productGet(id: string): Promise<ApiResult<Product>>;
  productUpdate(id: string, body: UpdateProductRequest): Promise<ApiResult<Product>>;
  productDelete(id: string): Promise<ApiResult<void>>;
  productRestore(id: string): Promise<ApiResult<Product>>;
  productRefresh(id: string): Promise<ApiResult<Product>>;
  stockList(query?: { include_depleted?: boolean | null }): Promise<ApiResult<StockListResponse>>;
  stockCreate(body: CreateStockRequest): Promise<ApiResult<StockBatch>>;
  stockGet(id: string): Promise<ApiResult<StockBatch>>;
  stockUpdate(id: string, body: UpdateStockRequest): Promise<ApiResult<StockBatch>>;
  stockListBatchEvents(
    id: string,
    query?: { before_created_at?: string | null; before_id?: string | null; limit?: number | null }
  ): Promise<ApiResult<StockEventListResponse>>;
  stockConsume(body: ConsumeRequest): Promise<ApiResult<ConsumeResponse>>;
  stockDelete(id: string): Promise<ApiResult<void>>;
  stockRestore(id: string): Promise<ApiResult<StockBatch>>;
  unitsList(): Promise<ApiResult<Unit[]>>;
  remindersList(query?: {
    after_fire_at?: string | null;
    after_id?: string | null;
    limit?: number | null;
  }): Promise<ApiResult<ReminderListResponse>>;
  remindersPresent(id: string): Promise<ApiResult<void>>;
  remindersOpen(id: string): Promise<ApiResult<void>>;
  remindersAck(id: string): Promise<ApiResult<void>>;
}

export class ApiFailure extends Error {
  constructor(
    public readonly status: number,
    message = `Request failed with HTTP ${status}`,
    public readonly code: string | null = null
  ) {
    super(message);
  }
}

export function defaultServerUrl(location: BrowserLocationLike | string = ''): string {
  if (typeof location === 'string') {
    return trimTrailingSlashes(location);
  }
  return trimTrailingSlashes(`${location.origin ?? ''}${webBasePath(location.pathname)}`);
}

export function currentHousehold(me: MeResponse): HouseholdSummary | null {
  return me.current_household ?? me.currentHousehold ?? null;
}

export function createBrowserSessionStorage(
  localStorage: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>,
  location: BrowserLocationLike | string
): SessionStorage {
  return {
    read() {
      return {
        serverUrl:
          localStorage.getItem('quartermaster.serverUrl')?.trim() || defaultServerUrl(location),
        browserDeviceId: localStorage.getItem('quartermaster.browserDeviceId')
      };
    },
    write(session) {
      localStorage.setItem('quartermaster.serverUrl', session.serverUrl);
      if (session.browserDeviceId) {
        localStorage.setItem('quartermaster.browserDeviceId', session.browserDeviceId);
      }
    },
    clear() {}
  };
}

export class QuartermasterSession {
  private session: StoredSession;
  private refreshInFlight: Promise<boolean> | null = null;
  private browserDeviceRegistrationInFlight: Promise<void> | null = null;

  constructor(
    private readonly storage: SessionStorage,
    private readonly transport: SessionTransport
  ) {
    this.session = storage.read();
    this.transport.configure(this.session);
  }

  snapshot(): StoredSession {
    return { ...this.session };
  }

  setServerUrl(serverUrl: string): void {
    this.session = {
      ...this.session,
      serverUrl: serverUrl.trim() || defaultServerUrl()
    };
    this.persist();
  }

  async login(username: string, password: string): Promise<void> {
    const result = await this.transport.login({ username, password });
    unwrap(result);
    await this.ensureBrowserDeviceRegistered();
  }

  onboardingStatus() {
    return this.transport.onboardingStatus().then(unwrap);
  }

  async createOnboardingHousehold(
    username: string,
    password: string,
    householdName: string,
    timezone: string
  ): Promise<void> {
    const result = await this.transport.createOnboardingHousehold({
      username,
      password,
      household_name: householdName,
      timezone
    });
    unwrap(result);
    await this.ensureBrowserDeviceRegistered();
  }

  async logout(): Promise<void> {
    await this.transport.logout().catch(() => undefined);
    this.persist();
  }

  me(): Promise<MeResponse> {
    return this.authed(() => this.transport.me());
  }

  switchHousehold(householdId: string): Promise<MeResponse> {
    return this.authed(() => this.transport.switchHousehold({ household_id: householdId }));
  }

  locationsList(): Promise<Location[]> {
    return this.authed(() => this.transport.locationsList());
  }

  locationsCreate(body: CreateLocationRequest): Promise<Location> {
    return this.authed(() => this.transport.locationsCreate(body));
  }

  locationsUpdate(id: string, body: UpdateLocationRequest): Promise<Location> {
    return this.authed(() => this.transport.locationsUpdate(id, body));
  }

  locationsDelete(id: string): Promise<void> {
    return this.authed(() => this.transport.locationsDelete(id));
  }

  productSearch(query: {
    q: string;
    limit?: number | null;
    include_deleted?: boolean | null;
  }): Promise<ProductSearchResponse> {
    return this.authed(() => this.transport.productSearch(query));
  }

  productList(query?: {
    q?: string | null;
    limit?: number | null;
    include_deleted?: boolean | null;
  }): Promise<ProductSearchResponse> {
    return this.authed(() => this.transport.productList(query));
  }

  productByBarcode(barcode: string): Promise<BarcodeLookupResponse> {
    return this.authed(() => this.transport.productByBarcode(barcode));
  }

  productCreate(body: CreateProductRequest): Promise<Product> {
    return this.authed(() => this.transport.productCreate(body));
  }

  productGet(id: string): Promise<Product> {
    return this.authed(() => this.transport.productGet(id));
  }

  productUpdate(id: string, body: UpdateProductRequest): Promise<Product> {
    return this.authed(() => this.transport.productUpdate(id, body));
  }

  productDelete(id: string): Promise<void> {
    return this.authed(() => this.transport.productDelete(id));
  }

  productRestore(id: string): Promise<Product> {
    return this.authed(() => this.transport.productRestore(id));
  }

  productRefresh(id: string): Promise<Product> {
    return this.authed(() => this.transport.productRefresh(id));
  }

  stockList(query?: { include_depleted?: boolean | null }): Promise<StockListResponse> {
    return this.authed(() => this.transport.stockList(query));
  }

  stockCreate(body: CreateStockRequest): Promise<StockBatch> {
    return this.authed(() => this.transport.stockCreate(body));
  }

  stockGet(id: string): Promise<StockBatch> {
    return this.authed(() => this.transport.stockGet(id));
  }

  stockUpdate(id: string, body: UpdateStockRequest): Promise<StockBatch> {
    return this.authed(() => this.transport.stockUpdate(id, body));
  }

  stockListBatchEvents(
    id: string,
    query?: { before_created_at?: string | null; before_id?: string | null; limit?: number | null }
  ): Promise<StockEventListResponse> {
    return this.authed(() => this.transport.stockListBatchEvents(id, query));
  }

  stockConsume(body: ConsumeRequest): Promise<ConsumeResponse> {
    return this.authed(() => this.transport.stockConsume(body));
  }

  stockDelete(id: string): Promise<void> {
    return this.authed(() => this.transport.stockDelete(id));
  }

  stockRestore(id: string): Promise<StockBatch> {
    return this.authed(() => this.transport.stockRestore(id));
  }

  unitsList(): Promise<Unit[]> {
    return this.authed(() => this.transport.unitsList());
  }

  remindersList(query?: {
    after_fire_at?: string | null;
    after_id?: string | null;
    limit?: number | null;
  }): Promise<ReminderListResponse> {
    return this.authed(async () => {
      await this.ensureBrowserDeviceRegistered();
      return this.transport.remindersList(query);
    });
  }

  remindersPresent(id: string): Promise<void> {
    return this.authed(async () => {
      await this.ensureBrowserDeviceRegistered();
      return this.transport.remindersPresent(id);
    });
  }

  remindersOpen(id: string): Promise<void> {
    return this.authed(async () => {
      await this.ensureBrowserDeviceRegistered();
      return this.transport.remindersOpen(id);
    });
  }

  remindersAck(id: string): Promise<void> {
    return this.authed(async () => {
      await this.ensureBrowserDeviceRegistered();
      return this.transport.remindersAck(id);
    });
  }

  private async authed<T>(run: () => Promise<ApiResult<T>>): Promise<T> {
    let result = await run();
    if (result.response?.status === 401) {
      const refreshed = await this.refreshOnce();
      if (refreshed) {
        result = await run();
      }
    }
    return unwrap(result);
  }

  private async refreshOnce(): Promise<boolean> {
    this.refreshInFlight ??= this.refreshTokens().finally(() => {
      this.refreshInFlight = null;
    });
    return this.refreshInFlight;
  }

  private async refreshTokens(): Promise<boolean> {
    const result = await this.transport.refresh();
    if (!result.data || result.response?.status === 401) {
      this.persist();
      return false;
    }
    this.persist();
    return true;
  }

  private async ensureBrowserDeviceRegistered(): Promise<void> {
    if (!this.transport.registerDevice) {
      return;
    }
    this.browserDeviceRegistrationInFlight ??= this.registerBrowserDevice().finally(() => {
      this.browserDeviceRegistrationInFlight = null;
    });
    await this.browserDeviceRegistrationInFlight;
  }

  private async registerBrowserDevice(): Promise<void> {
    const registerDevice = this.transport.registerDevice;
    if (!registerDevice) {
      return;
    }
    if (!this.session.browserDeviceId) {
      this.session = {
        ...this.session,
        browserDeviceId: stableBrowserDeviceId()
      };
      this.persist();
    }
    const deviceId = this.session.browserDeviceId;
    if (!deviceId) {
      return;
    }
    await registerDevice
      .call(this.transport, {
        device_id: deviceId,
        platform: 'web',
        push_token: null,
        push_authorization: 'denied',
        app_version: null
      })
      .catch(() => undefined);
  }

  private persist(): void {
    this.storage.write(this.session);
    this.transport.configure(this.session);
  }
}

function stableBrowserDeviceId(): string {
  const randomUUID = globalThis.crypto?.randomUUID?.bind(globalThis.crypto);
  if (randomUUID) {
    return `web-${randomUUID()}`;
  }
  return `web-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export function unwrap<T>(result: ApiResult<T>): T {
  if (result.data !== undefined) {
    return result.data;
  }
  if (result.response?.status === 204) {
    return undefined as T;
  }
  const body = result.error as { code?: string; message?: string } | undefined;
  throw new ApiFailure(result.response?.status ?? 0, body?.message, body?.code ?? null);
}
