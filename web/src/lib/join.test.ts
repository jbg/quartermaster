import { describe, expect, it } from 'vitest';
import { quartermasterJoinUrl, quartermasterNativeUrl, readJoinDetails } from './join';

describe('join links', () => {
  it('reads invite and server query parameters', () => {
    const details = readJoinDetails(
      new URLSearchParams('invite=ABCD1234&server=https%3A%2F%2Fexample.com')
    );

    expect(details).toEqual({
      invite: 'ABCD1234',
      server: 'https://example.com'
    });
  });

  it('builds the native custom-scheme fallback link', () => {
    expect(
      quartermasterNativeUrl({
        invite: 'AB CD',
        server: 'https://quartermaster.example.com'
      })
    ).toBe('quartermaster://join?invite=AB%20CD&server=https%3A%2F%2Fquartermaster.example.com');
  });

  it('builds server-only pairing links', () => {
    expect(
      quartermasterNativeUrl({
        invite: '',
        server: 'https://quartermaster.example.com'
      })
    ).toBe('quartermaster://join?server=https%3A%2F%2Fquartermaster.example.com');
  });

  it('omits the query string when no details are present', () => {
    expect(quartermasterJoinUrl({ invite: '', server: '' })).toBe('quartermaster://join');
  });
});
