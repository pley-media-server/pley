package app.pley.android.common

import java.net.URI
import java.net.URISyntaxException

/** pley's web port, for addresses typed without one. */
const val DEFAULT_PORT = 7539

/** A server opened before. */
data class SavedServer(val url: String, val name: String, val id: String?, val lastUsed: Long)

/**
 * Where an address leads, which orders a server's addresses: a home
 * network's first, then a VPN's (Tailscale), then the internet's.
 */
enum class AddressKind { HOME, VPN, INTERNET }

/** A server that answered on the local network. */
data class FoundServer(val url: String, val name: String, val id: String?, val version: String)

/**
 * One row of the server list: saved, found on the network, or both.
 * [lookalike]: found, not saved, and named like a saved server it isn't.
 */
data class ServerEntry(
    val url: String,
    val name: String,
    val id: String?,
    val saved: Boolean,
    val found: Boolean,
    val lookalike: Boolean = false,
)

/** The rules the desktop app's picker follows too (apps/desktop/src/servers.ts). */
object Servers {
    /**
     * `nas`, `nas:8080`, `192.168.1.20` or `https://pley.example.com/` as a
     * server's address (its origin); `null` when it can't be one. Without a
     * scheme it's http, and without a port pley's.
     */
    fun normalizeUrl(input: String): String? {
        val text = input.trim()
        if (text.isEmpty() || text.any { it.isWhitespace() }) return null
        val hasScheme = Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://").containsMatchIn(text)
        val uri = try {
            URI(if (hasScheme) text else "http://$text")
        } catch (_: URISyntaxException) {
            return null
        }
        val scheme = uri.scheme?.lowercase() ?: return null
        val host = uri.host?.lowercase() ?: return null
        if (scheme != "http" && scheme != "https") return null
        val port = when {
            uri.port != -1 -> uri.port
            !hasScheme -> DEFAULT_PORT
            else -> -1
        }
        // A port nothing listens on (0, or past 65535): a typo.
        if (port != -1 && port !in 1..65535) return null
        val defaultPort = (scheme == "http" && port == 80) || (scheme == "https" && port == 443)
        return if (port == -1 || defaultPort) "$scheme://$host" else "$scheme://$host:$port"
    }

    /**
     * Whether [host] is this device, on its local network (a private address,
     * a name without dots, `.local`…) or a Tailscale name: where plain http is
     * how pley is reached, and the network itself is trusted.
     */
    fun isLocalHost(host: String): Boolean {
        val h = host.lowercase().removePrefix("[").removeSuffix("]")
        if (h == "localhost" || (!h.contains('.') && !h.contains(':'))) return true
        if (Regex("""\.(local|lan|home|internal|localdomain|home\.arpa|ts\.net)$""").containsMatchIn(h)) return true
        val v4 = Regex("""^(\d+)\.(\d+)\.\d+\.\d+$""").find(h)
        if (v4 != null) {
            val a = v4.groupValues[1].toInt()
            val b = v4.groupValues[2].toInt()
            return a == 10 || a == 127 || (a == 172 && b in 16..31) || (a == 192 && b == 168) ||
                (a == 169 && b == 254) || (a == 100 && b in 64..127)
        }
        return h == "::1" || Regex("^f[cd][0-9a-f]{0,2}:").containsMatchIn(h) || Regex("^fe[89ab][0-9a-f]?:").containsMatchIn(h)
    }

    /**
     * Where [url] leads: Tailscale's addresses and names are a VPN's, the
     * rest of [isLocalHost] a home network's, anything else the internet's.
     */
    fun kindOf(url: String): AddressKind {
        val host = try {
            URI(url).host
        } catch (_: URISyntaxException) {
            null
        }?.lowercase()?.removePrefix("[")?.removeSuffix("]") ?: return AddressKind.INTERNET
        val v4 = Regex("""^100\.(\d+)\.\d+\.\d+$""").find(host)
        if (host.endsWith(".ts.net") || (v4 != null && v4.groupValues[1].toInt() in 64..127)) return AddressKind.VPN
        return if (isLocalHost(host)) AddressKind.HOME else AddressKind.INTERNET
    }

    /** Whether a connection to [url] travels unencrypted beyond the local network: passwords and all. */
    fun unencrypted(url: String): Boolean {
        val uri = try {
            URI(url)
        } catch (_: URISyntaxException) {
            return false
        }
        return uri.scheme?.lowercase() == "http" && !isLocalHost(uri.host ?: return false)
    }

    /**
     * The addresses to try, in turn, for what someone typed: a name on the
     * internet typed without `http://` is tried with https first (pley behind
     * a proxy with a certificate), then with plain http on pley's port.
     */
    fun candidates(input: String): List<String> {
        val url = normalizeUrl(input) ?: return emptyList()
        val text = input.trim()
        val typedScheme = Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://").containsMatchIn(text)
        val typedPort = Regex("""^[^/]*:\d+$""").matches(text.substringBefore('/'))
        val host = URI(url).host ?: return listOf(url)
        if (typedScheme || typedPort || isLocalHost(host)) return listOf(url)
        return listOf("https://$host", url)
    }

    /** The address to show for a server: its host and, unless it's pley's, its port. */
    fun displayAddress(url: String): String {
        val uri = try {
            URI(url)
        } catch (_: URISyntaxException) {
            return url
        }
        val host = uri.host ?: return url
        return if (uri.port == -1 || (uri.scheme == "http" && uri.port == DEFAULT_PORT)) host else "$host:${uri.port}"
    }

    private fun same(aUrl: String, aId: String?, bUrl: String, bId: String?): Boolean =
        (aId != null && aId == bId) || aUrl == bUrl

    /** Saved servers, most recently opened first, then the ones found on the network that aren't saved. */
    fun merge(saved: List<SavedServer>, found: List<FoundServer>): List<ServerEntry> {
        val entries = saved.sortedByDescending { it.lastUsed }.map { s ->
            ServerEntry(s.url, s.name, s.id, saved = true, found = found.any { same(s.url, s.id, it.url, it.id) })
        }.toMutableList()
        for (f in found) {
            if (entries.none { same(it.url, it.id, f.url, f.id) }) {
                val lookalike = saved.any { it.name.trim().equals(f.name.trim(), ignoreCase = true) }
                entries.add(ServerEntry(f.url, f.name, f.id, saved = false, found = true, lookalike = lookalike))
            }
        }
        return entries
    }

    /** The server to open by itself at start: the one opened last. */
    fun lastUsed(saved: List<SavedServer>): SavedServer? = saved.maxByOrNull { it.lastUsed }

    /** `servers` with `server` in it, replacing one with the same address or id (a server found at a new address). */
    fun withServer(servers: List<SavedServer>, server: SavedServer): List<SavedServer> =
        servers.filter { it.url != server.url && (server.id == null || it.id != server.id) } + server
}
