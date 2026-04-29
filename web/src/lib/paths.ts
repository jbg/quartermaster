export interface BrowserLocationLike {
  origin?: string;
  pathname?: string;
}

const WEB_ROUTE_ROOTS = new Set(['batches', 'join', 'products', 'reminders', 'settings']);

export function trimTrailingSlashes(value: string): string {
  return value.replace(/\/+$/, '');
}

export function webBasePath(pathname = ''): string {
  const normalized = `/${pathname}`.replace(/\/+/g, '/');
  const segments = normalized.split('/').filter(Boolean);
  const routeIndex = segments.findIndex((segment) => WEB_ROUTE_ROOTS.has(segment));
  if (routeIndex >= 0) {
    segments.splice(routeIndex);
  }
  return segments.length > 0 ? `/${segments.join('/')}` : '';
}

export function appPath(path: string, location: BrowserLocationLike | URL): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return `${webBasePath(location.pathname)}${normalizedPath}`;
}
