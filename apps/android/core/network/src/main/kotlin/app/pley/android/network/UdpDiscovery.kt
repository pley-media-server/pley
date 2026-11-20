package app.pley.android.network

import app.pley.android.common.DEFAULT_PORT
import app.pley.android.common.LocalNetwork
import java.io.Closeable
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.SocketTimeoutException
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** A datagram that came in. */
internal class Datagram(val bytes: ByteArray, val length: Int, val from: InetAddress)

/** A UDP socket on one network, as discovery uses it. */
internal interface DatagramPort : Closeable {
    /** Sends [payload] to [to]; throws when the network or the system refuses. */
    fun send(payload: ByteArray, to: InetSocketAddress)

    /** The next datagram, or `null` when none came for a moment; ends when the coroutine is canceled. */
    suspend fun receive(): Datagram?
}

/** A broadcast socket bound to [lan] (or to the default network without one). */
internal class SocketPort(lan: Lan?, private val io: CoroutineDispatcher = Dispatchers.IO) : DatagramPort {
    private val socket = DatagramSocket().apply {
        try {
            broadcast = true
            lan?.bindDatagram?.invoke(this)
        } catch (e: Exception) {
            close()
            throw e
        }
    }
    private val buffer = ByteArray(2048)

    override fun send(payload: ByteArray, to: InetSocketAddress) {
        socket.send(DatagramPacket(payload, payload.size, to))
    }

    override suspend fun receive(): Datagram? = withContext(io) {
        // Short waits, so the search ends on time: a blocked receive can't be canceled.
        socket.soTimeout = POLL_MS
        val packet = DatagramPacket(buffer, buffer.size)
        try {
            socket.receive(packet)
            Datagram(packet.data.copyOf(packet.length), packet.length, packet.address)
        } catch (_: SocketTimeoutException) {
            null
        }
    }

    override fun close() = socket.close()

    private companion object {
        const val POLL_MS = 100
    }
}

/**
 * Discovery by broadcast (the server's side is apps/server/src/discovery.rs):
 * `pley?` to UDP [port] on every Wi-Fi and Ethernet network, sent at each
 * time of [schedule] (a lost datagram is common on Wi-Fi), the answers
 * listened for until [windowMs] after the first. Each server comes once,
 * by its id, whichever network and address it answered on. A refusal by the
 * system (EPERM: Android 17's local network access, Fire OS 16's nearby
 * devices) is [DiscoveryEvent.Blocked]; the search goes on elsewhere.
 */
internal class UdpDiscovery(
    private val networks: LanNetworks,
    private val open: (Lan?) -> DatagramPort = { SocketPort(it) },
    private val port: Int = DEFAULT_PORT,
    private val schedule: List<Long> = SCHEDULE_MS,
    private val windowMs: Long = WINDOW_MS,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : Discovery {
    override fun search(): Flow<DiscoveryEvent> = channelFlow {
        val seen = mutableSetOf<String>()
        val blocked = AtomicBoolean(false)
        suspend fun refused(e: Throwable): Boolean {
            if (!LocalNetwork.isPermissionBlock(e)) return false
            if (blocked.compareAndSet(false, true)) send(DiscoveryEvent.Blocked)
            return true
        }

        // Without a known home network, the default one (an emulator's, a phone's hotspot).
        val lans: List<Lan?> = networks.current().ifEmpty { listOf(null) }
        withTimeoutOrNull(windowMs) {
            coroutineScope {
                for (lan in lans) {
                    launch(io) {
                        val socket = try {
                            open(lan)
                        } catch (e: IOException) {
                            refused(e)
                            return@launch
                        } catch (e: SecurityException) {
                            refused(e)
                            return@launch
                        }
                        socket.use {
                            val targets = targetsOf(lan)
                            launch {
                                var last = 0L
                                for (at in schedule) {
                                    delay(at - last)
                                    last = at
                                    for (target in targets) {
                                        try {
                                            socket.send(REQUEST, InetSocketAddress(target, port))
                                        } catch (e: IOException) {
                                            // That address takes no broadcast: the others may.
                                            refused(e)
                                        } catch (e: SecurityException) {
                                            refused(e)
                                        }
                                    }
                                }
                            }
                            while (true) {
                                val datagram = try {
                                    socket.receive() ?: continue
                                } catch (e: IOException) {
                                    refused(e)
                                    break
                                }
                                val server = app.pley.android.common.Discovery.readAnswer(datagram.bytes, datagram.length, datagram.from)
                                    ?: continue
                                val fresh = synchronized(seen) { seen.add(server.id ?: server.url) }
                                if (fresh) send(DiscoveryEvent.Found(server))
                            }
                        }
                    }
                }
            }
        }
        send(DiscoveryEvent.Done)
    }

    private fun targetsOf(lan: Lan?): List<InetAddress> = buildList {
        lan?.addresses?.forEach { add(it.broadcast) }
        add(ALL)
    }.distinct()

    companion object {
        private val REQUEST = "pley?".toByteArray()
        private val ALL: InetAddress = InetAddress.getByAddress(byteArrayOf(-1, -1, -1, -1))

        /** When `pley?` goes out, in ms from the start. */
        val SCHEDULE_MS = listOf(0L, 200L, 600L)

        /** How long answers are listened for, from the start. */
        const val WINDOW_MS = 1_500L
    }
}
