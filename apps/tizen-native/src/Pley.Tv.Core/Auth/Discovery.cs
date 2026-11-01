using System;
using System.Collections.Generic;
using System.Linq;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Net;

namespace Pley.Tv.Core.Auth;

/// <summary>A server that answered on the local network: by broadcast, or a search of the network.</summary>
/// <param name="Id">The server's own id, the same at every address; null from a search, which doesn't ask.</param>
public sealed record FoundServer(string Url, string Name, string? Id, string Version);

/// <summary>A UDP socket as discovery uses it: UdpClient on the TV, a fake in tests.</summary>
public interface IDatagramPort : IDisposable
{
    /// <summary>Sends <paramref name="payload"/> to <paramref name="to"/>; throws when the network refuses.</summary>
    void Send(byte[] payload, IPEndPoint to);

    /// <summary>The next datagram; cancelled by <paramref name="cancel"/>.</summary>
    Task<(byte[] Bytes, IPEndPoint From)> Receive(CancellationToken cancel);
}

/// <summary>A broadcast socket on every network of the TV.</summary>
public sealed class UdpPort : IDatagramPort
{
    readonly UdpClient client = new UdpClient(0) { EnableBroadcast = true };

    public void Send(byte[] payload, IPEndPoint to) => client.Send(payload, payload.Length, to);

    public async Task<(byte[] Bytes, IPEndPoint From)> Receive(CancellationToken cancel)
    {
        // UdpClient of .NET Standard takes no token: the wait ends with the socket, closed when cancelled.
        using (cancel.Register(client.Dispose))
        {
            try
            {
                var got = await client.ReceiveAsync().ConfigureAwait(false);
                return (got.Buffer, got.RemoteEndPoint);
            }
            catch (ObjectDisposedException) when (cancel.IsCancellationRequested)
            {
                throw new OperationCanceledException(cancel);
            }
            catch (SocketException) when (cancel.IsCancellationRequested)
            {
                throw new OperationCanceledException(cancel);
            }
        }
    }

    public void Dispose() => client.Dispose();
}

/// <summary>
/// Finding Pley servers by broadcast (the server's side is apps/server/src/discovery.rs):
/// <c>pley?</c> to UDP port 7539 on every network of the TV and to 255.255.255.255, sent three
/// times (a datagram lost on Wi-Fi is common), the answers listened for 1.5 s from the start.
/// Each server comes once, by its id, whichever network and address it answered on. The Android
/// app's <c>UdpDiscovery</c>.
/// </summary>
public static class Discovery
{
    /// <summary>When <c>pley?</c> goes out, in ms from the start.</summary>
    public static readonly long[] ScheduleMs = { 0, 200, 600 };

    /// <summary>How long answers are listened for, from the start.</summary>
    public const int WindowMs = 1_500;

    static readonly byte[] Request = Encoding.ASCII.GetBytes("pley?");

    /// <summary>The server behind an answer from <paramref name="from"/>, if it is one: <c>{"pley":1,"port":7539,"name":…,"id":…,"version":…}</c>.</summary>
    public static FoundServer? ReadAnswer(byte[] bytes, IPEndPoint from)
    {
        JsonElement o;
        try
        {
            using var doc = JsonDocument.Parse(bytes);
            o = doc.RootElement.Clone();
        }
        catch (JsonException)
        {
            return null;
        }
        if (Json.Number(o, "pley") != 1) return null;
        var port = Json.Number(o, "port") ?? 0;
        if (port <= 0 || port > 65535 || port != Math.Floor(port)) return null;
        var address = from.Address.IsIPv4MappedToIPv6 ? from.Address.MapToIPv4() : from.Address;
        var host = address.AddressFamily == AddressFamily.InterNetworkV6
            ? $"[{address.ToString().Split('%')[0]}]"
            : address.ToString();
        // What a datagram says is shown: as plain text, and the address when it has no name.
        var name = Plain.Of(Json.String(o, "name"));
        var id = Json.String(o, "id");
        return new FoundServer(
            $"http://{host}:{(int)port}",
            name.Length > 0 ? name : host,
            string.IsNullOrEmpty(id) ? null : id,
            Json.String(o, "version") ?? "");
    }

    /// <summary>
    /// Broadcasts and collects the answers: <paramref name="onFound"/> hears each server as it
    /// answers, on any thread; the answer is all of them, by name. <paramref name="lans"/> are
    /// the networks to broadcast on (<see cref="Lans.Current"/> without), <paramref name="open"/>
    /// the socket (<see cref="UdpPort"/> without).
    /// </summary>
    public static async Task<IReadOnlyList<FoundServer>> Search(Action<FoundServer>? onFound, CancellationToken stop,
        IReadOnlyList<LanAddress>? lans = null, Func<IDatagramPort>? open = null, int port = Addresses.DefaultPort)
    {
        var found = new List<FoundServer>();
        var seen = new HashSet<string>();
        using var window = CancellationTokenSource.CreateLinkedTokenSource(stop);
        window.CancelAfter(WindowMs);
        IDatagramPort socket;
        try
        {
            socket = (open ?? (() => new UdpPort()))();
        }
        catch (SocketException)
        {
            // No network: nothing found.
            return found;
        }
        using (socket)
        {
            var targets = (lans ?? Lans.Current()).Select(l => l.Broadcast).Append(IPAddress.Broadcast).Distinct().ToList();
            var sending = Send(socket, targets, port, window.Token);
            while (!window.IsCancellationRequested)
            {
                (byte[] Bytes, IPEndPoint From) datagram;
                try
                {
                    datagram = await socket.Receive(window.Token).ConfigureAwait(false);
                }
                catch (OperationCanceledException)
                {
                    break;
                }
                catch (SocketException)
                {
                    // A refusal by the network (an unreachable port's answer): others may still answer.
                    continue;
                }
                var server = ReadAnswer(datagram.Bytes, datagram.From);
                if (server == null || !seen.Add(server.Id ?? server.Url)) continue;
                found.Add(server);
                onFound?.Invoke(server);
            }
            try
            {
                await sending.ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                // The window closed before the last send.
            }
        }
        return found.OrderBy(s => s.Name.ToLowerInvariant()).ToList();
    }

    static async Task Send(IDatagramPort socket, IReadOnlyList<IPAddress> targets, int port, CancellationToken cancel)
    {
        long last = 0;
        foreach (var at in ScheduleMs)
        {
            if (at > last) await Task.Delay(TimeSpan.FromMilliseconds(at - last), cancel).ConfigureAwait(false);
            last = at;
            foreach (var target in targets)
            {
                try
                {
                    socket.Send(Request, new IPEndPoint(target, port));
                }
                catch (SocketException)
                {
                    // That address takes no broadcast: the others may.
                }
                catch (ObjectDisposedException)
                {
                    return;
                }
            }
        }
    }
}
