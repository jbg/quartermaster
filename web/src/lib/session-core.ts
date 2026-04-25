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

export interface StockBatch {
  id: string;
  product?: {
    id?: string;
    name?: string;
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
  kind?: string;
  title: string;
  body: string;
  fire_at?: string;
  fireAt?: string;
  household_timezone?: string;
  householdTimezone?: string;
  household_fire_local_at?: string;
  householdFireLocalAt?: string;
  expires_on?: string | null;
  expiresOn?: string | null;
  batch_id?: string;
  batchId?: string;
  product_id?: string;
  productId?: string;
  location_id?: string;
  locationId?: string;
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

export interface UpdateProductRequest {
  name?: string | null;
  brand?: string | null;
  family?: UnitFamily | null;
  preferred_unit?: string | null;
  image_url?: string | null;
}

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

export interface UpdateStockRequest {
  quantity?: string | null;
  location_id?: string | null;
  expires_on?: string | null;
  opened_on?: string | null;
  note?: string | null;
}

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
  accessToken: string | null;
  refreshToken: string | null;
}

export interface SessionTransport {
  configure(session: StoredSession): void;
  login(body: { username: string; password: string }): Promise<ApiResult<TokenPair>>;
  register(body: {
    username: string;
    password: string;
    email?: string | null;
    invite_code?: string | null;
  }): Promise<ApiResult<TokenPair>>;
  refresh(body: { refresh_token: string }): Promise<ApiResult<TokenPair>>;
  logout(): Promise<ApiResult<void>>;
  me(): Promise<ApiResult<MeResponse>>;
  switchHousehold(body: { household_id: string }): Promise<ApiResult<MeResponse>>;
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

export interface BrowserLocationLike {
  origin: string;
  pathname?: string;
}

const WEB_ROUTE_ROOTS = new Set(['join', 'products', 'settings']);

function trimTrailingSlashes(value: string): string {
  return value.replace(/\/+$/, '');
}

function ingressBasePath(pathname = ''): string {
  const normalized = `/${pathname}`.replace(/\/+/g, '/');
  const segments = normalized.split('/').filter(Boolean);
  const routeIndex = segments.findIndex((segment) => WEB_ROUTE_ROOTS.has(segment));
  if (routeIndex >= 0) {
    segments.splice(routeIndex);
  }
  return segments.length > 0 ? `/${segments.join('/')}` : '';
}

export function defaultServerUrl(location: BrowserLocationLike | string = ''): string {
  if (typeof location === 'string') {
    return trimTrailingSlashes(location);
  }
  return trimTrailingSlashes(`${location.origin}${ingressBasePath(location.pathname)}`);
}

export function tokenPairAccess(pair: TokenPair): string {
  return pair.access_token ?? pair.accessToken ?? '';
}

export function tokenPairRefresh(pair: TokenPair): string {
  return pair.refresh_token ?? pair.refreshToken ?? '';
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
        accessToken: localStorage.getItem('quartermaster.accessToken'),
        refreshToken: localStorage.getItem('quartermaster.refreshToken')
      };
    },
    write(session) {
      localStorage.setItem('quartermaster.serverUrl', session.serverUrl);
      if (session.accessToken) {
        localStorage.setItem('quartermaster.accessToken', session.accessToken);
      } else {
        localStorage.removeItem('quartermaster.accessToken');
      }
      if (session.refreshToken) {
        localStorage.setItem('quartermaster.refreshToken', session.refreshToken);
      } else {
        localStorage.removeItem('quartermaster.refreshToken');
      }
    },
    clear() {
      localStorage.removeItem('quartermaster.accessToken');
      localStorage.removeItem('quartermaster.refreshToken');
    }
  };
}

export class QuartermasterSession {
  private session: StoredSession;
  private refreshInFlight: Promise<boolean> | null = null;

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
    this.storeTokenResult(result);
  }

  async register(
    username: string,
    password: string,
    email: string,
    inviteCode: string
  ): Promise<void> {
    const result = await this.transport.register({
      username,
      password,
      email: email || null,
      invite_code: inviteCode || null
    });
    this.storeTokenResult(result);
  }

  async logout(): Promise<void> {
    await this.transport.logout().catch(() => undefined);
    this.session = {
      ...this.session,
      accessToken: null,
      refreshToken: null
    };
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

  remindersList(query?: {
    after_fire_at?: string | null;
    after_id?: string | null;
    limit?: number | null;
  }): Promise<ReminderListResponse> {
    return this.authed(() => this.transport.remindersList(query));
  }

  remindersPresent(id: string): Promise<void> {
    return this.authed(() => this.transport.remindersPresent(id));
  }

  remindersOpen(id: string): Promise<void> {
    return this.authed(() => this.transport.remindersOpen(id));
  }

  remindersAck(id: string): Promise<void> {
    return this.authed(() => this.transport.remindersAck(id));
  }

  private async authed<T>(run: () => Promise<ApiResult<T>>): Promise<T> {
    let result = await run();
    if (result.response?.status === 401 && this.session.refreshToken) {
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
    const refreshToken = this.session.refreshToken;
    if (!refreshToken) {
      return false;
    }
    const result = await this.transport.refresh({ refresh_token: refreshToken });
    if (!result.data || result.response?.status === 401) {
      this.session = {
        ...this.session,
        accessToken: null,
        refreshToken: null
      };
      this.persist();
      return false;
    }
    this.applyTokens(result.data);
    return true;
  }

  private storeTokenResult(result: ApiResult<TokenPair>): void {
    const pair = unwrap(result);
    this.applyTokens(pair);
  }

  private applyTokens(pair: TokenPair): void {
    this.session = {
      ...this.session,
      accessToken: tokenPairAccess(pair),
      refreshToken: tokenPairRefresh(pair)
    };
    this.persist();
  }

  private persist(): void {
    this.storage.write(this.session);
    this.transport.configure(this.session);
  }
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
