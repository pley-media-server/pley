package app.pley.android.fixtures

import app.pley.android.model.CastKind
import app.pley.android.model.DeviceKind
import app.pley.android.model.DeviceProfileName
import app.pley.android.model.PleyJson
import app.pley.android.model.RemoteDevice
import app.pley.android.model.ScanPhase
import app.pley.android.model.ServerEvent
import java.io.IOException
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArraySet
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.LinkedBlockingQueue
import mockwebserver3.MockResponse
import mockwebserver3.MockResponseBody
import okio.BufferedSink

/**
 * GET /api/events: the server's events as Server-Sent Events, to every
 * open stream. Each stream gets a keep-alive comment every
 * [FixtureServer.Options.keepAliveMs] (the server's 15 s) and ends when its
 * access token does, as the server's. Every
 * [FixtureServer.Options.scanEveryMs] (30 s) one library is "scanned":
 * scan_started, its progress, scan_finished and library_changed. Writes
 * add their own (library_changed, sessions_changed), and tests can send
 * any with [FixtureServer.emit].
 *
 * A stream that names its device (`receiver`, `device_name`,
 * `device_kind`, `casts`) makes it a receiver the profile's remotes cast
 * to while it is open, as the server's (docs/API.md, "Casting"): several
 * streams with one id count once, it goes with the last, a profile has at
 * most [MAX_RECEIVERS], and one appearing or going sends `sessions_changed`.
 * A home member's remotes that ask (`household=1`) list the household's
 * other profiles' receivers that switch profile (`switch`) too, and a cast
 * to one carries a switch ticket ([issueTicket]) the device switches to the
 * caster's profile with ([redeem]).
 */
internal class EventHub(private val state: State) {
    private val streams = CopyOnWriteArraySet<LinkedBlockingQueue<String>>()

    /** The profile each stream is for. */
    private val owners = ConcurrentHashMap<LinkedBlockingQueue<String>, Long>()

    /** A receiver while its streams are open: what its newest one says, and since when. */
    private class Open(var device: RemoteDevice, var streams: Int, var connectedAt: Long)

    /** The receivers open, by profile and receiver id. */
    private val receivers = HashMap<Long, LinkedHashMap<String, Open>>()

    /** How many streams have opened, which orders receivers that connect in the same millisecond. */
    private var opened = 0L
    private var scheduler: ScheduledExecutorService? = null
    private var scans = 0

    fun start() {
        val every = state.options.scanEveryMs
        if (every <= 0) return
        scheduler = Executors.newSingleThreadScheduledExecutor { task ->
            Thread(task, "FixtureServer scans").apply { isDaemon = true }
        }.also { it.scheduleWithFixedDelay(::scan, every, every, TimeUnit.MILLISECONDS) }
    }

    /** Stops the scans and ends every stream. */
    fun stop() {
        scheduler?.shutdownNow()
        scheduler = null
        drop()
    }

    /** Ends every stream: the server stops listening. */
    fun drop() {
        streams.forEach { it.offer(END) }
    }

    /** Open streams: tests wait for one before sending. */
    val listeners: Int get() = streams.size

    fun emit(event: ServerEvent) {
        val text = PleyJson.encodeToString(ServerEvent.Serializer, event)
        streams.forEach { it.offer(text) }
    }

    /** Sends [event] to the streams of profile [profileId] only. */
    fun emitTo(profileId: Long, event: ServerEvent) {
        val text = PleyJson.encodeToString(ServerEvent.Serializer, event)
        streams.forEach { if (owners[it] == profileId) it.offer(text) }
    }

    /**
     * [profileId]'s receivers, one per device (the one that connected
     * last), by name, those without one last: GET /api/remote/devices.
     */
    fun devices(profileId: Long): List<RemoteDevice> = synchronized(receivers) {
        receivers[profileId].orEmpty().values
            .groupBy { it.device.deviceId }
            .values.map { open -> open.maxBy { it.connectedAt }.device }
            .sortedWith(compareBy<RemoteDevice>({ it.name == null }, { it.name?.lowercase() }, { it.receiverId }))
    }

    /**
     * GET /api/remote/devices?household=1 for [profileId]: its own
     * receivers, then the household's other profiles' that switch to the
     * caster's profile (`switch` in their `casts`), each with the profile it
     * is on (its name as "Who's watching?" says it). A device on two
     * profiles at once (while it switches) is listed once, as the caller's
     * own; by name, those without one last, as the server sorts them.
     */
    fun householdDevices(profileId: Long): List<RemoteDevice> {
        val others = state.household.profiles.values.filter { it.id != profileId }
        val all = devices(profileId) + others.flatMap { other ->
            devices(other.id).filter { CastKind.Switch in it.casts }.map { it.copy(profile = DeviceProfileName(other.id, other.name)) }
        }
        val seen = HashSet<String>()
        return all.filter { seen.add(it.deviceId) }
            .sortedWith(compareBy<RemoteDevice>({ it.name == null }, { it.name?.lowercase() }, { it.receiverId }))
    }

    /** What receiver [id] of [profileId] plays when cast to; `null` when it isn't open. */
    fun casts(profileId: Long, id: String): List<CastKind>? = synchronized(receivers) { receivers[profileId]?.get(id)?.device?.casts }

    /** Which device receiver [id] of [profileId] is, while it is open. */
    fun deviceOf(profileId: Long, id: String): String? = synchronized(receivers) { receivers[profileId]?.get(id)?.device?.deviceId }

    /** What a cast's switch ticket lets its receiver do: device [deviceId], on profile [from], switches to [to], once. */
    private class Ticket(val deviceId: String, val from: Long, val to: Long, val issuedAtMs: Long)

    /** The tickets waiting, by their hash, as the server keeps them, oldest first. */
    private val tickets = LinkedHashMap<String, Ticket>()

    /** A ticket for device [deviceId], on profile [from], to switch to profile [to] once within [TICKET_TTL_MS] of [nowMs]. */
    fun issueTicket(deviceId: String, from: Long, to: Long, nowMs: Long): String = synchronized(tickets) {
        tickets.values.removeAll { nowMs - it.issuedAtMs >= TICKET_TTL_MS }
        if (tickets.size >= MAX_TICKETS) tickets.remove(tickets.keys.first())
        val ticket = "pley_st_" + Base64.getUrlEncoder().withoutPadding().encodeToString(ByteArray(24).also(random::nextBytes))
        tickets[hash(ticket)] = Ticket(deviceId, from, to, nowMs)
        ticket
    }

    /**
     * Spends [ticket]: whether it lets device [deviceId], on profile [from],
     * switch to profile [to] at [nowMs]. Shown for anything else, it is
     * spent too: it was copied.
     */
    fun redeem(ticket: String, deviceId: String, from: Long, to: Long, nowMs: Long): Boolean = synchronized(tickets) {
        val t = tickets.remove(hash(ticket)) ?: return false
        nowMs - t.issuedAtMs < TICKET_TTL_MS && t.deviceId == deviceId && t.from == from && t.to == to
    }

    private val random = SecureRandom()

    private fun hash(ticket: String): String =
        Base64.getEncoder().encodeToString(MessageDigest.getInstance("SHA-256").digest(ticket.toByteArray(Charsets.UTF_8)))

    /** When casts went to each receiver in the last second, by profile and receiver id. */
    private val castsSent = HashMap<Pair<Long, String>, ArrayDeque<Long>>()

    /** Counts a cast to receiver [id] of [profileId] at [nowMs]: `false` past [CASTS_PER_SECOND] a second, as on the server. */
    fun takeCast(profileId: Long, id: String, nowMs: Long): Boolean = synchronized(receivers) {
        val sent = castsSent.getOrPut(profileId to id) { ArrayDeque() }
        while (sent.isNotEmpty() && nowMs - sent.first() >= 1_000) sent.removeFirst()
        if (sent.size >= CASTS_PER_SECOND) return false
        sent.addLast(nowMs)
        true
    }

    /**
     * The receiver [call]'s stream names, if it names a valid one: its id
     * (1 to 128 letters, digits, `.`, `_`, `:` and `-`), its device as
     * sessions key it, its name (else the User-Agent), its kind and what it
     * casts, the kinds this server knows only.
     */
    private fun receiverOf(call: Call, caller: Caller): RemoteDevice? {
        val id = call.query("receiver")?.takeIf { RECEIVER_ID.matches(it) } ?: return null
        val name = (call.query("device_name") ?: call.header("User-Agent"))?.trim()?.take(DEVICE_NAME_MAX)?.ifEmpty { null }
        val kind = call.query("device_kind")?.let { k -> DeviceKind.entries.firstOrNull { it.wire == k } }?.takeIf { it != DeviceKind.Unknown }
        val casts = call.query("casts").orEmpty().split(',').mapNotNull { k -> CastKind.entries.firstOrNull { it.wire == k.trim() && it != CastKind.Unknown } }
        return RemoteDevice(
            receiverId = id,
            deviceId = caller.grant?.let { "app-${it.id}" } ?: "receiver-$id",
            name = name,
            kind = kind ?: DeviceKind.Other,
            casts = casts.distinct().sortedBy { it.ordinal },
        )
    }

    /** Counts a stream of [profileId] as [device] until [close]d; `false` when the profile has too many receivers. */
    private fun openReceiver(profileId: Long, device: RemoteDevice): Boolean {
        val changed = synchronized(receivers) {
            val theirs = receivers.getOrPut(profileId) { LinkedHashMap() }
            val open = theirs[device.receiverId]
            when {
                open != null -> {
                    open.streams++
                    open.connectedAt = ++opened
                    (open.device != device).also { open.device = device }
                }
                theirs.size >= MAX_RECEIVERS -> return false
                else -> {
                    theirs[device.receiverId] = Open(device, 1, ++opened)
                    true
                }
            }
        }
        if (changed) emitTo(profileId, ServerEvent.SessionsChanged)
        return true
    }

    private fun closeReceiver(profileId: Long, id: String) {
        val gone = synchronized(receivers) {
            val theirs = receivers[profileId] ?: return
            val open = theirs[id] ?: return
            open.streams--
            if (open.streams > 0) return
            theirs.remove(id)
            if (theirs.isEmpty()) receivers.remove(profileId)
            true
        }
        if (gone) emitTo(profileId, ServerEvent.SessionsChanged)
    }

    private fun scan() {
        val library = state.catalog.libraries[scans++ % state.catalog.libraries.size]
        val total = library.itemCount.toLong()
        emit(ServerEvent.ScanStarted(library.id))
        emit(ServerEvent.ScanProgress(library.id, ScanPhase.Walking, 0, total))
        emit(ServerEvent.ScanProgress(library.id, ScanPhase.Probing, total / 2, total))
        emit(ServerEvent.ScanProgress(library.id, ScanPhase.Metadata, total, total))
        emit(ServerEvent.ScanFinished(library.id, added = 0, updated = 0, removed = 0))
        emit(ServerEvent.LibraryChanged(library.id))
    }

    /** The stream for [caller], until its token ends, it goes away or the server stops; a receiver while it lasts when [call] names one. */
    fun response(call: Call, caller: Caller): MockResponse {
        val queue = LinkedBlockingQueue<String>()
        val receiver = receiverOf(call, caller)
        val body = object : MockResponseBody {
            override val contentLength: Long = -1L

            override fun writeTo(sink: BufferedSink) {
                owners[queue] = caller.profile.id
                streams += queue
                val counted = receiver != null && openReceiver(caller.profile.id, receiver)
                try {
                    // The headers go out with the first comment.
                    chunk(sink, ": connected\n\n")
                    while (true) {
                        val left = caller.expiresAtMs?.let { it - state.nowMs() } ?: Long.MAX_VALUE
                        if (left <= 0) break
                        val next = queue.poll(minOf(left, state.options.keepAliveMs), TimeUnit.MILLISECONDS)
                        when {
                            next === END -> break
                            next != null -> chunk(sink, "data: $next\n\n")
                            else -> chunk(sink, ": keep-alive\n\n")
                        }
                    }
                    sink.writeUtf8("0\r\n\r\n")
                    sink.flush()
                } catch (_: IOException) {
                    // The app went away.
                } catch (_: InterruptedException) {
                    Thread.currentThread().interrupt()
                } finally {
                    streams -= queue
                    owners -= queue
                    if (counted) closeReceiver(caller.profile.id, receiver!!.receiverId)
                }
            }
        }
        return MockResponse.Builder()
            .code(200)
            .body(body)
            // Chunked, written here, so the stream can end cleanly.
            .removeHeader("Content-Length")
            .setHeader("Transfer-Encoding", "chunked")
            .setHeader("Content-Type", "text/event-stream")
            .setHeader("Cache-Control", "no-cache")
            .build()
    }

    private fun chunk(sink: BufferedSink, text: String) {
        val bytes = text.toByteArray(Charsets.UTF_8)
        sink.writeUtf8(Integer.toHexString(bytes.size)).writeUtf8("\r\n").write(bytes).writeUtf8("\r\n")
        sink.flush()
    }

    private companion object {
        /** Ends a stream. Compared by identity. */
        val END = String(charArrayOf('e', 'n', 'd'))

        /** The most receivers a profile has at once, as on the server. */
        const val MAX_RECEIVERS = 32

        /** The most casts to one receiver a second, as on the server. */
        const val CASTS_PER_SECOND = 10

        /** How long a cast's switch ticket lasts, and the most waiting at once, as on the server. */
        const val TICKET_TTL_MS = 60_000L
        const val MAX_TICKETS = 256

        /** What the server keeps of a device's name. */
        const val DEVICE_NAME_MAX = 64

        val RECEIVER_ID = Regex("[A-Za-z0-9._:-]{1,128}")
    }
}
