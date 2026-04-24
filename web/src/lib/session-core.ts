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

export interface StockBatch {
  id: string;
  product?: {
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
  location?: {
    name?: string;
  } | null;
  location_name?: string | null;
  locationName?: string | null;
  expires_on?: string | null;
  expiresOn?: string | null;
  depleted_at?: string | null;
  depletedAt?: string | null;
}

export interface StockListResponse {
  items?: StockBatch[];
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
  stockList(): Promise<ApiResult<StockListResponse>>;
}

export class ApiFailure extends Error {
  constructor(
    public readonly status: number,
    message = `Request failed with HTTP ${status}`
  ) {
    super(message);
  }
}

export function defaultServerUrl(locationOrigin = ''): string {
  return locationOrigin || '';
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
  locationOrigin: string
): SessionStorage {
  return {
    read() {
      return {
        serverUrl:
          localStorage.getItem('quartermaster.serverUrl')?.trim() || defaultServerUrl(locationOrigin),
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

  stockList(): Promise<StockListResponse> {
    return this.authed(() => this.transport.stockList());
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
