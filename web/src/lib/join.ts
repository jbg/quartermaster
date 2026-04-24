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
  const invite = encodeURIComponent(details.invite);
  const server = encodeURIComponent(details.server);
  if (!invite && !server) {
    return 'quartermaster://join';
  }
  return `quartermaster://join?invite=${invite}&server=${server}`;
}
