import { browserCookie } from '$lib/api';
import type { QuartermasterSession } from '$lib/session-core';

export interface ApiResult<T> {
  data?: T;
  error?: unknown;
  response?: Response;
}

export function unwrapGenerated<T>(result: ApiResult<T>): T {
  if (result.data !== undefined) {
    return result.data;
  }
  const message =
    typeof result.error === 'object' &&
    result.error !== null &&
    'message' in result.error &&
    typeof result.error.message === 'string'
      ? result.error.message
      : `Request failed${result.response ? ` with HTTP ${result.response.status}` : ''}`;
  throw new Error(message);
}

export async function apiFetch<T>(
  session: QuartermasterSession,
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const baseUrl = session.snapshot().serverUrl.replace(/\/+$/, '');
  const headers = new Headers(options.headers);
  if (options.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  if (!['GET', 'HEAD', 'OPTIONS'].includes((options.method ?? 'GET').toUpperCase())) {
    const csrf = browserCookie('qm_csrf');
    if (csrf) {
      headers.set('X-QM-CSRF', csrf);
    }
  }
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    credentials: 'include',
    headers
  });
  if (!response.ok) {
    let message = `Request failed with HTTP ${response.status}`;
    try {
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Keep the HTTP status message.
    }
    throw new Error(message);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export function jsonPreview(value: unknown): string {
  if (value === null || value === undefined) {
    return 'None';
  }
  if (typeof value === 'string') {
    return value;
  }
  return JSON.stringify(value, null, 2);
}

export function lineKey(value: string | null | undefined, index: number): string {
  return value && value.trim() ? value : String(index);
}
