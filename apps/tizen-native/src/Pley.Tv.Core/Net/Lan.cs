using System;
using System.Collections.Generic;
using System.Linq;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;

namespace Pley.Tv.Core.Net;

/// <summary>An IPv4 address of the TV on a home network, with its network's prefix (<c>192.168.1.20/24</c>).</summary>
public sealed record LanAddress(IPAddress Address, int PrefixLength)
{
    uint Ip
    {
        get
        {
            var b = Address.GetAddressBytes();
            return ((uint)b[0] << 24) | ((uint)b[1] << 16) | ((uint)b[2] << 8) | b[3];
        }
    }

    static IPAddress Of(uint n) => new IPAddress(new[] { (byte)(n >> 24), (byte)(n >> 16), (byte)(n >> 8), (byte)n });

    /// <summary>Where a broadcast to the whole network goes: <c>192.168.1.255</c>.</summary>
    public IPAddress Broadcast
    {
        get
        {
            var prefix = Math.Min(PrefixLength, 32);
            var mask = prefix <= 0 ? 0u : uint.MaxValue << (32 - prefix);
            return Of(Ip | ~mask);
        }
    }

    /// <summary>
    /// The other addresses of the TV's /24 (or of its network, when that is smaller):
    /// <c>192.168.1.1</c> to <c>.254</c> but the TV's own. A bigger network is still searched by
    /// its /24 only: 254 addresses are a few seconds' work, 65,534 are not.
    /// </summary>
    public IReadOnlyList<IPAddress> Neighbours()
    {
        var prefix = Math.Max(24, Math.Min(30, PrefixLength));
        var mask = uint.MaxValue << (32 - prefix);
        var ip = Ip;
        var network = ip & mask;
        var size = 1u << (32 - prefix);
        var list = new List<IPAddress>();
        for (var i = 1u; i < size - 1; i++)
        {
            if (network + i != ip) list.Add(Of(network + i));
        }
        return list;
    }
}

/// <summary>The home networks the TV is on now.</summary>
public static class Lans
{
    /// <summary>
    /// The TV's IPv4 addresses on its wired and wireless networks that are up, with their
    /// prefixes; none when the system won't say. A network whose prefix isn't known counts as a /24.
    /// </summary>
    public static IReadOnlyList<LanAddress> Current()
    {
        var found = new List<LanAddress>();
        try
        {
            foreach (var nic in NetworkInterface.GetAllNetworkInterfaces())
            {
                if (nic.OperationalStatus != OperationalStatus.Up) continue;
                if (nic.NetworkInterfaceType is NetworkInterfaceType.Loopback or NetworkInterfaceType.Tunnel or NetworkInterfaceType.Ppp) continue;
                foreach (var a in nic.GetIPProperties().UnicastAddresses)
                {
                    if (a.Address.AddressFamily != AddressFamily.InterNetwork || IPAddress.IsLoopback(a.Address)) continue;
                    var lan = new LanAddress(a.Address, PrefixOf(a));
                    if (!found.Contains(lan)) found.Add(lan);
                }
            }
        }
        catch (Exception e) when (e is NetworkInformationException or PlatformNotSupportedException or NotImplementedException)
        {
            // Not told: a search finds what broadcasts reach.
        }
        return found;
    }

    static int PrefixOf(UnicastIPAddressInformation a)
    {
        try
        {
            var mask = a.IPv4Mask?.GetAddressBytes();
            if (mask == null || mask.Length != 4) return 24;
            var bits = mask.Sum(b => Enumerable.Range(0, 8).Count(i => (b & (1 << i)) != 0));
            return bits == 0 ? 24 : bits;
        }
        catch (Exception e) when (e is PlatformNotSupportedException or NotImplementedException)
        {
            return 24;
        }
    }
}
