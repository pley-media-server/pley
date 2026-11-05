/** pley's web port, for addresses typed without one. */
export const DEFAULT_PORT = 7539;

interface Parsed {
  scheme: string;
  host: string;
  port: number | null;
}

/** Scheme, host and port of an absolute http(s) address; `null` otherwise. */
function parse(url: string): Parsed | null {
  const m = /^([a-zA-Z][a-zA-Z0-9+.-]*):\/\/(\[[0-9a-fA-F:.]+\]|[^/?#:@\s]+)(?::(\d{1,5}))?(?:[/?#].*)?$/.exec(url.trim());
  if (!m) return null;
  const scheme = m[1]!.toLowerCase();
  if (scheme !== "http" && scheme !== "https") return null;
  const port = m[3] ? Number(m[3]) : null;
  if (port !== null && (port < 1 || port > 65535)) return null;
  return { scheme, host: m[2]!.toLowerCase(), port };
}

function write(p: Parsed): string {
  const defaultPort = p.port === null || (p.scheme === "http" && p.port === 80) || (p.scheme === "https" && p.port === 443);
  return defaultPort ? `${p.scheme}://${p.host}` : `${p.scheme}://${p.host}:${p.port}`;
}

/**
 * The origin of `url` as browsers write it (`https://pley.example.com`,
 * `http://nas:7539`): scheme and host in lower case, the port only when it
 * isn't the scheme's own. A server's issuer is its origin, so tokens are
 * matched against this.
 */
export function origin(url: string): string | null {
  const p = parse(url);
  return p ? write(p) : null;
}

/**
 * `nas`, `nas:8080`, `192.168.1.20` or `https://pley.example.com/` as a
 * server's address; `null` when it can't be one. Without a scheme it's
 * http, and without a port pley's.
 */
export function normalizeServer(input: string): string | null {
  const text = input.trim();
  if (!text || /\s/.test(text)) return null;
  const hasScheme = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(text);
  const p = parse(hasScheme ? text : `http://${text}`);
  if (!p) return null;
  if (p.port === null && !hasScheme) p.port = DEFAULT_PORT;
  return write(p);
}

/**
 * Whether `host` is this device, on its local network (a private address,
 * a name without dots, `.local`…) or a Tailscale name: where plain http is
 * how pley is reached, and the network itself is trusted.
 */
export function isLocalHost(host: string): boolean {
  const h = host.toLowerCase().replace(/^\[/, "").replace(/\]$/, "");
  if (h === "localhost" || (h.indexOf(".") < 0 && h.indexOf(":") < 0)) return true;
  if (/\.(local|lan|home|internal|localdomain|home\.arpa|ts\.net)$/.test(h)) return true;
  const v4 = /^(\d+)\.(\d+)\.\d+\.\d+$/.exec(h);
  if (v4) {
    const a = Number(v4[1]);
    const b = Number(v4[2]);
    return (
      a === 10 || a === 127 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 169 && b === 254) ||
      (a === 100 && b >= 64 && b <= 127)
    );
  }
  return h === "::1" || /^f[cd][0-9a-f]{0,2}:/.test(h) || /^fe[89ab][0-9a-f]?:/.test(h);
}

/**
 * Whether `url`'s server is on the home network itself (a private address,
 * a name without dots, `.local`…), which accepts a connection at once or
 * never: not a Tailscale address, which may be far away (the Android app's
 * LocalRoute.isHomeHost).
 */
export function atHome(url: string): boolean {
  const p = parse(url);
  if (!p) return false;
  const h = p.host.replace(/^\[/, "").replace(/\]$/, "");
  if (/\.ts\.net$/.test(h)) return false;
  const v4 = /^(\d+)\.(\d+)\.\d+\.\d+$/.exec(h);
  if (v4 && Number(v4[1]) === 100 && Number(v4[2]) >= 64 && Number(v4[2]) <= 127) return false;
  return isLocalHost(h);
}

/** Whether a connection to `url` would travel unencrypted beyond the local network. */
export function unencrypted(url: string): boolean {
  const p = parse(url);
  return !!p && p.scheme === "http" && !isLocalHost(p.host);
}

/** The address to show for a server: its host and, unless it's pley's, its port. */
export function displayAddress(url: string): string {
  const p = parse(url);
  if (!p) return url;
  return p.port === null || (p.scheme === "http" && p.port === DEFAULT_PORT) ? p.host : `${p.host}:${p.port}`;
}

/**
 * The addresses to try, in turn, for what someone typed: a name on the
 * internet typed without `http://` is tried with https first (pley behind
 * a proxy with a certificate), then with plain http on pley's port.
 */
export function candidates(input: string): string[] {
  const url = normalizeServer(input);
  if (!url) return [];
  const text = input.trim();
  const typedScheme = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(text);
  const typedPort = /^[^/]*:\d+$/.test(text.split("/")[0]!);
  const p = parse(url)!;
  if (typedScheme || typedPort || isLocalHost(p.host)) return [url];
  return [`https://${p.host}`, url];
}
