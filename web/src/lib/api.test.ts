import { afterEach, describe, expect, it, vi } from 'vitest';
import { browserCookie, withCsrfHeader } from './api';

describe('browser API transport helpers', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads browser cookies by name', () => {
    vi.stubGlobal('document', { cookie: 'theme=dark; qm_csrf=csrf-token; other=value' });

    expect(browserCookie('qm_csrf')).toBe('csrf-token');
    expect(browserCookie('missing')).toBeNull();
  });

  it('adds CSRF header to unsafe browser requests', () => {
    vi.stubGlobal('document', { cookie: 'qm_csrf=csrf-token' });

    const request = withCsrfHeader(
      new Request('https://example.com/api/v1/stock', { method: 'POST' })
    );

    expect(request.headers.get('X-QM-CSRF')).toBe('csrf-token');
  });

  it('does not add CSRF header to safe browser requests', () => {
    vi.stubGlobal('document', { cookie: 'qm_csrf=csrf-token' });

    const request = withCsrfHeader(
      new Request('https://example.com/api/v1/stock', { method: 'GET' })
    );

    expect(request.headers.has('X-QM-CSRF')).toBe(false);
  });
});
