export interface JoinDetails {
  invite: string;
  server: string;
}

export function readJoinDetails(searchParams: URLSearchParams): JoinDetails {
  return {
    invite: searchParams.get('invite') ?? '',
    server: searchParams.get('server') ?? ''
  };
}

export function quartermasterJoinUrl(details: JoinDetails): string {
  return quartermasterNativeUrl(details);
}

export function quartermasterServerUrl(server: string): string {
  if (!server) {
    return 'quartermaster://server';
  }
  return `quartermaster://server?server=${encodeURIComponent(server)}`;
}

export function quartermasterNativeUrl(details: JoinDetails): string {
  const params: string[] = [];
  if (details.invite) {
    params.push(`invite=${encodeURIComponent(details.invite)}`);
  }
  if (details.server) {
    params.push(`server=${encodeURIComponent(details.server)}`);
  }
  const query = params.join('&');
  if (!query) {
    return 'quartermaster://join';
  }
  return `quartermaster://join?${query}`;
}
