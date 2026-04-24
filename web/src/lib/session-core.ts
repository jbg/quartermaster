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
  stockList(query?: { include_depleted?: boolean | null }): Promise<ApiResult<StockListResponse>>;
  stockGet(id: string): Promise<ApiResult<StockBatch>>;
  stockListBatchEvents(id: string, query?: { before_created_at?: string | null; before_id?: string | null; limit?: number | null }): Promise<ApiResult<StockEventListResponse>>;
  stockConsume(body: ConsumeRequest): Promise<ApiResult<ConsumeResponse>>;
  stockDelete(id: string): Promise<ApiResult<void>>;
  stockRestore(id: string): Promise<ApiResult<StockBatch>>;
  remindersList(query?: { after_fire_at?: string | null; after_id?: string | null; limit?: number | null }): Promise<ApiResult<ReminderListResponse>>;
  remindersPresent(id: string): Promise<ApiResult<void>>;
  remindersOpen(id: string): Promise<ApiResult<void>>;
  remindersAck(id: string): Promise<ApiResult<void>>;
}

export class ApiFailure extends Error {
  constructor(
    public readonly status: number,
    message = `Request failed with HTTP ${status}`
  ) {
    super(message);
  }
}

export interface BrowserLocationLike {
  origin: string;
  pathname?: string;
}

const WEB_ROUTE_SEGMENTS = new Set(['join']);

function trimTrailingSlashes(value: string): string {
  return value.replace(/\/+$/, '');
}

function ingressBasePath(pathname = ''): string {
  const normalized = `/${pathname}`.replace(/\/+/g, '/');
  const segments = normalized.split('/').filter(Boolean);
  const last = segments.at(-1);
  if (last && WEB_ROUTE_SEGMENTS.has(last)) {
    segments.pop();
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

  async register(username: string, password: string, email: string, inviteCode: string): Promise<void> {
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

  stockList(query?: { include_depleted?: boolean | null }): Promise<StockListResponse> {
    return this.authed(() => this.transport.stockList(query));
  }

  stockGet(id: string): Promise<StockBatch> {
    return this.authed(() => this.transport.stockGet(id));
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

  remindersList(query?: { after_fire_at?: string | null; after_id?: string | null; limit?: number | null }): Promise<ReminderListResponse> {
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
  throw new ApiFailure(result.response?.status ?? 0);
}
