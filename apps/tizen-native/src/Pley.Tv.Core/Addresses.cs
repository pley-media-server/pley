using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;

namespace Pley.Tv.Core;

/// <summary>
/// Server addresses as people type them and as the app keeps them: the Android app's
/// <c>Servers</c> and <c>Addresses</c>, which the web TV app's <c>auth/addresses.ts</c> answers
/// the same in most cases (packages/tv/test/fixtures/addresses.json).
/// </summary>
public static class Addresses
{
    /// <summary>Pley's web port, for addresses typed without one.</summary>
    public const int DefaultPort = 7539;

    /// <summary>What a TV can't sign in over (the Android app's words).</summary>
    public const string Unencrypted =
        "this server's address isn't on your network and its connection isn't encrypted: open it by its https address to sign in";

    // Digits are spelled [0-9]: .NET's \d takes every script's digits, and `\z` ends the text where JavaScript's `$` does.
    // A user name before the host is passed over, as Java's URI does for the Android app; a host
    // name is ASCII letters, digits, dots and hyphens, so `my_nas` isn't one there either.
    static readonly Regex Absolute = new Regex(
        @"^([a-zA-Z][a-zA-Z0-9+.-]*)://(?:[^/?#@\s\[\]]*@)?(\[[0-9a-fA-F:.]+\]|[a-zA-Z0-9.-]+)(?::([0-9]{1,5}))?(?:[/?#].*)?\z",
        RegexOptions.CultureInvariant);

    static readonly Regex Scheme = new Regex(@"^[a-zA-Z][a-zA-Z0-9+.-]*://", RegexOptions.CultureInvariant);
    static readonly Regex TypedPort = new Regex(@"^[^/]*:[0-9]+\z", RegexOptions.CultureInvariant);
    static readonly Regex Whitespace = new Regex(@"\s", RegexOptions.CultureInvariant);
    static readonly Regex LocalSuffix = new Regex(@"\.(local|lan|home|internal|localdomain|home\.arpa|ts\.net)\z", RegexOptions.CultureInvariant);
    static readonly Regex IPv4 = new Regex(@"^([0-9]+)\.([0-9]+)\.[0-9]+\.[0-9]+\z", RegexOptions.CultureInvariant);
    static readonly Regex UniqueLocal = new Regex(@"^f[cd][0-9a-f]{0,2}:", RegexOptions.CultureInvariant);
    static readonly Regex LinkLocal = new Regex(@"^fe[89ab][0-9a-f]?:", RegexOptions.CultureInvariant);

    sealed class Parsed
    {
        public string Scheme = "";
        public string Host = "";
        public int? Port;
    }

    /// <summary>Scheme, host and port of an absolute http(s) address; null otherwise.</summary>
    static Parsed? Parse(string url)
    {
        var m = Absolute.Match(url.Trim());
        if (!m.Success) return null;
        var scheme = m.Groups[1].Value.ToLowerInvariant();
        if (scheme != "http" && scheme != "https") return null;
        int? port = m.Groups[3].Success ? int.Parse(m.Groups[3].Value) : null;
        if (port != null && (port < 1 || port > 65535)) return null;
        return new Parsed { Scheme = scheme, Host = m.Groups[2].Value.ToLowerInvariant(), Port = port };
    }

    /// <summary>An IPv6 host without its brackets ("[::1]" is "::1").</summary>
    static string Unbracket(string host)
    {
        if (host.StartsWith("[")) host = host.Substring(1);
        return host.EndsWith("]") ? host.Substring(0, host.Length - 1) : host;
    }

    static string Write(Parsed p)
    {
        var defaultPort = p.Port == null || (p.Scheme == "http" && p.Port == 80) || (p.Scheme == "https" && p.Port == 443);
        return defaultPort ? $"{p.Scheme}://{p.Host}" : $"{p.Scheme}://{p.Host}:{p.Port}";
    }

    /// <summary>
    /// The origin of <paramref name="url"/> as browsers write it (<c>https://pley.example.com</c>,
    /// <c>http://nas:7539</c>): scheme and host in lower case, the port only when it isn't the
    /// scheme's own. A server's issuer is its origin, so tokens are matched against this.
    /// </summary>
    public static string? Origin(string url)
    {
        var p = Parse(url);
        return p == null ? null : Write(p);
    }

    /// <summary>
    /// <c>nas</c>, <c>nas:8080</c>, <c>192.168.1.20</c> or <c>https://pley.example.com/</c> as a
    /// server's address; null when it can't be one. Without a scheme it's http, and without a
    /// port Pley's.
    /// </summary>
    public static string? NormalizeServer(string input)
    {
        var text = input.Trim();
        if (text.Length == 0 || Whitespace.IsMatch(text)) return null;
        var hasScheme = Scheme.IsMatch(text);
        var p = Parse(hasScheme ? text : "http://" + text);
        if (p == null) return null;
        if (p.Port == null && !hasScheme) p.Port = DefaultPort;
        return Write(p);
    }

    /// <summary>
    /// Whether <paramref name="host"/> is this device, on its local network (a private address,
    /// a name without dots, <c>.local</c>…) or a Tailscale name: where plain http is how Pley is
    /// reached, and the network itself is trusted.
    /// </summary>
    public static bool IsLocalHost(string host)
    {
        var h = Unbracket(host.ToLowerInvariant());
        if (h == "localhost" || (h.IndexOf('.') < 0 && h.IndexOf(':') < 0)) return true;
        if (LocalSuffix.IsMatch(h)) return true;
        var v4 = IPv4.Match(h);
        if (v4.Success)
        {
            // Numbers too long for an int aren't private addresses, as Number() makes them in JavaScript.
            var a = double.Parse(v4.Groups[1].Value, System.Globalization.CultureInfo.InvariantCulture);
            var b = double.Parse(v4.Groups[2].Value, System.Globalization.CultureInfo.InvariantCulture);
            return a == 10 || a == 127 || (a == 172 && b >= 16 && b <= 31) || (a == 192 && b == 168) || (a == 169 && b == 254) ||
                (a == 100 && b >= 64 && b <= 127);
        }
        return h == "::1" || UniqueLocal.IsMatch(h) || LinkLocal.IsMatch(h);
    }

    /// <summary>
    /// Whether <paramref name="url"/>'s server is on the home network itself (a private address,
    /// a name without dots, <c>.local</c>…), which accepts a connection at once or never: not a
    /// Tailscale address, which may be far away, nor this device (the Android app's
    /// LocalRoute.isHomeHost).
    /// </summary>
    public static bool AtHome(string url)
    {
        var p = Parse(url);
        if (p == null) return false;
        var h = Unbracket(p.Host);
        if (h.Length == 0) return false;
        var literal = h.IndexOf(':') >= 0 || h.All(c => (c >= '0' && c <= '9') || c == '.');
        if (!literal) return h != "localhost" && !h.EndsWith(".ts.net") && IsLocalHost(h);
        if (!System.Net.IPAddress.TryParse(h, out var address) || System.Net.IPAddress.IsLoopback(address)) return false;
        var b = address.GetAddressBytes();
        if (address.AddressFamily == System.Net.Sockets.AddressFamily.InterNetwork)
            return b[0] == 10 || (b[0] == 172 && b[1] >= 16 && b[1] <= 31) || (b[0] == 192 && b[1] == 168) || (b[0] == 169 && b[1] == 254);
        // Link-local (fe80::/10), the old site-local (fec0::/10) and unique local (fc00::/7), as Java reads them.
        return address.IsIPv6LinkLocal || address.IsIPv6SiteLocal || (b[0] & 0xfe) == 0xfc;
    }

    /// <summary>Whether a connection to <paramref name="url"/> would travel unencrypted beyond the local network.</summary>
    public static bool IsUnencrypted(string url)
    {
        var p = Parse(url);
        return p != null && p.Scheme == "http" && !IsLocalHost(p.Host);
    }

    /// <summary>The address to show for a server: its host and, unless it's Pley's, its port.</summary>
    public static string DisplayAddress(string url)
    {
        var p = Parse(url);
        if (p == null) return url;
        return p.Port == null || (p.Scheme == "http" && p.Port == DefaultPort) ? p.Host : $"{p.Host}:{p.Port}";
    }

    /// <summary>
    /// The addresses to try, in turn, for what someone typed: a name on the internet typed
    /// without <c>http://</c> is tried with https first (Pley behind a proxy with a
    /// certificate), then with plain http on Pley's port.
    /// </summary>
    public static IReadOnlyList<string> Candidates(string input)
    {
        var url = NormalizeServer(input);
        if (url == null) return new string[0];
        var text = input.Trim();
        var typedScheme = Scheme.IsMatch(text);
        var typedPort = TypedPort.IsMatch(text.Split('/')[0]);
        var p = Parse(url)!;
        if (typedScheme || typedPort || IsLocalHost(p.Host)) return new[] { url };
        return new[] { $"https://{p.Host}", url };
    }
}
