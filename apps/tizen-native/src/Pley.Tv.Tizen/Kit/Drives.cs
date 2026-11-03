using System;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
using Pley.Tv.Core;
using Tizen.System;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The drive plugged into the TV, as the TV mounts it ("/opt/media/USBDriveA1"): the first one
/// mounted, null while none is. It follows drives plugged in and pulled out as the app runs.
/// </summary>
sealed class Drives : IDisposable
{
    readonly SynchronizationContext? ui = SynchronizationContext.Current;
    readonly EventHandler changed;

    public Drives()
    {
        changed = (_, _) => Post(Look);
        try
        {
            StorageManager.SetChangedEvent(StorageArea.External, changed);
        }
        catch (Exception)
        {
            // Not told of drives coming and going: what is there now still shows.
        }
        Look();
    }

    /// <summary>The drive's folder; null while none is plugged in.</summary>
    public Signal<string?> Root { get; } = new Signal<string?>(null);

    void Look()
    {
        string? root = null;
        try
        {
            root = StorageManager.Storages
                .Where(s => s.StorageType == StorageArea.External && s.State is StorageState.Mounted or StorageState.MountedReadOnly)
                .Select(s => s.RootDirectory)
                .FirstOrDefault(r => !string.IsNullOrEmpty(r));
        }
        catch (Exception)
        {
            // The TV wouldn't say: none.
        }
        Root.Value = root;
    }

    /// <summary>
    /// The drive's make and model as it reports them ("Kingston DataTraveler 3.0"), from the disk
    /// the TV mounts at <paramref name="root"/>; null when it says nothing. The name given to the
    /// drive itself is out of reach: the app may not open the device to read it.
    /// </summary>
    public static string? Name(string root)
    {
        try
        {
            var device = File.ReadLines("/proc/self/mounts").Select(l => l.Split(' ')).FirstOrDefault(p => p.Length > 1 && p[1] == root)?[0];
            if (device == null || !device.StartsWith("/dev/", StringComparison.Ordinal)) return null;
            // sda1 is a part of the disk sda (mmcblk0p1 of mmcblk0), whose folder holds what the drive says of itself.
            var disk = Regex.Replace(Path.GetFileName(device), @"(?<=\d)p\d+$|(?<=[a-z])\d+$", "");
            string Said(string field)
            {
                var file = $"/sys/block/{disk}/device/{field}";
                return File.Exists(file) ? File.ReadAllText(file).Trim() : "";
            }
            var name = string.Join(" ", new[] { Said("vendor"), Said("model") }.Where(s => s.Length > 0));
            return name.Length > 0 ? name : null;
        }
        catch (Exception)
        {
            // The TV wouldn't say.
            return null;
        }
    }

    void Post(Action action)
    {
        if (ui != null) ui.Post(_ => action(), null);
        else action();
    }

    public void Dispose()
    {
        try
        {
            StorageManager.UnsetChangedEvent(StorageArea.External, changed);
        }
        catch (Exception)
        {
            // Never set.
        }
    }
}
