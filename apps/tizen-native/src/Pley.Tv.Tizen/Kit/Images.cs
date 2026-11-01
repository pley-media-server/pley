using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Net;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// Pictures from the server (the Android app's artwork loader): each address asked for at the
/// width of the box it fills, tried in order until one loads, kept in a cache on the TV, and an
/// address the server said doesn't exist remembered, so a missing poster costs one request. The
/// server wants the token for pictures and NUI can't send it, so the app downloads them and gives
/// NUI the file.
/// </summary>
sealed class Images
{
    /// <summary>Downloads at once: enough to fill a row while it scrolls, few enough not to starve the page's own requests.</summary>
    const int Parallel = 6;
    /// <summary>The cache's size on the TV before the oldest pictures go.</summary>
    const long CacheBytes = 200L * 1024 * 1024;
    /// <summary>A picture not in by then is given up.</summary>
    const int FetchMs = 20_000;
    /// <summary>A download is written a buffer of this at a time: under .NET's large objects.</summary>
    const int BufferBytes = 64 * 1024;

    readonly string dir;
    readonly SemaphoreSlim slots = new SemaphoreSlim(Parallel);
    readonly ConcurrentDictionary<string, bool> missing = new ConcurrentDictionary<string, bool>();
    readonly ConcurrentDictionary<string, Task<string?>> loading = new ConcurrentDictionary<string, Task<string?>>();
    readonly SynchronizationContext? ui = SynchronizationContext.Current;

    public Images(string cacheDir)
    {
        dir = Path.Combine(cacheDir, "images");
        Directory.CreateDirectory(dir);
        Task.Run(Trim);
    }

    /// <summary>The API pictures are fetched through (the token); set when the server changes.</summary>
    public Api? Api { get; set; }

    /// <summary>
    /// The first of <paramref name="candidates"/> that loads, at <paramref name="widthDp"/>, as a file
    /// for NUI: <paramref name="done"/> hears it on the UI thread (null when none loads).
    /// </summary>
    public void Load(IReadOnlyList<string> candidates, float widthDp, Action<string?> done, CancellationToken cancel = default)
    {
        var px = (int)Math.Ceiling(Ui.Dp(widthDp));
        _ = Run();

        async Task Run()
        {
            string? file = null;
            foreach (var candidate in candidates)
            {
                if (cancel.IsCancellationRequested) return;
                var url = Artwork.Sized(candidate, px);
                if (missing.ContainsKey(url)) continue;
                var asked = Profile.Start();
                try
                {
                    file = await Shared(url, cancel).ConfigureAwait(false);
                }
                catch (OperationCanceledException)
                {
                    return;
                }
                if (file != null) Profile.Picture(asked);
                if (file != null) break;
            }
            if (cancel.IsCancellationRequested) return;
            if (ui != null) ui.Post(_ => done(file), null);
            else done(file);
        }
    }

    /// <summary>
    /// <paramref name="url"/>'s download, shared with whoever asked for it first. It goes from the
    /// shared ones as it ends, however it ends: one cancelled and left there answered everyone who
    /// asked after with nothing, until the app started again. When the one who asked first gave up,
    /// another who still wants it asks again.
    /// </summary>
    async Task<string?> Shared(string url, CancellationToken cancel)
    {
        while (true)
        {
            var task = loading.GetOrAdd(url, u => Fetch(u, cancel));
            try
            {
                return await task.ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (!cancel.IsCancellationRequested)
            {
                // Another's download, given up: this one asks again.
            }
            finally
            {
                // Only this download: one asked again since may stand in its place.
                ((ICollection<KeyValuePair<string, Task<string?>>>)loading).Remove(new KeyValuePair<string, Task<string?>>(url, task));
            }
        }
    }

    /// <summary>The file for <paramref name="url"/>, null when it can't be had; cancelled only when <paramref name="cancel"/> is.</summary>
    async Task<string?> Fetch(string url, CancellationToken cancel)
    {
        var file = Path.Combine(dir, Hash(url) + ".img");
        if (File.Exists(file))
        {
            File.SetLastAccessTimeUtc(file, DateTime.UtcNow);
            return file;
        }
        var api = Api;
        if (api == null) return null;
        await slots.WaitAsync(cancel).ConfigureAwait(false);
        // Written whole, then renamed: a half-written picture is never shown, and one that failed goes.
        var part = file + ".part";
        try
        {
            // To the file as it comes, a buffer at a time: read whole into memory, a preview sheet of
            // a few MB took three times that of .NET's heap, a few at once 80 MB, and the app stopped
            // answering in the collections that followed.
            using var limit = CancellationTokenSource.CreateLinkedTokenSource(cancel);
            limit.CancelAfter(FetchMs);
            using var res = await api.Open(url, limit.Token).ConfigureAwait(false);
            long length;
            using (var to = new FileStream(part, FileMode.Create, FileAccess.Write, FileShare.None, BufferBytes, useAsync: true))
            {
                await res.Body.CopyToAsync(to, BufferBytes, limit.Token).ConfigureAwait(false);
                length = to.Length;
            }
            if (length == 0)
            {
                File.Delete(part);
                return null;
            }
            if (File.Exists(file)) File.Delete(part);
            else File.Move(part, file);
            return file;
        }
        catch (ApiException e) when (e.Status == 404)
        {
            missing[url] = true;
            return null;
        }
        catch (Exception) when (cancel.IsCancellationRequested)
        {
            // Given up by the one who asked, not timed out: others who want it ask again. While the
            // server hadn't answered yet, the transport says so as a NetworkException.
            throw new OperationCanceledException(cancel);
        }
        catch (Exception)
        {
            return null;
        }
        finally
        {
            slots.Release();
            try
            {
                File.Delete(part);
            }
            catch (Exception)
            {
                // Overwritten by the next download of the same picture.
            }
        }
    }

    static string Hash(string url)
    {
        using var sha = SHA1.Create();
        return BitConverter.ToString(sha.ComputeHash(Encoding.UTF8.GetBytes(url))).Replace("-", "");
    }

    /// <summary>Keeps the cache under its size: the pictures used longest ago go first.</summary>
    void Trim()
    {
        try
        {
            var files = new List<FileInfo>(new DirectoryInfo(dir).GetFiles("*.img"));
            long total = 0;
            foreach (var f in files) total += f.Length;
            if (total <= CacheBytes) return;
            files.Sort((a, b) => a.LastAccessTimeUtc.CompareTo(b.LastAccessTimeUtc));
            foreach (var f in files)
            {
                if (total <= CacheBytes * 3 / 4) break;
                total -= f.Length;
                f.Delete();
            }
        }
        catch (Exception)
        {
            // A cache that can't be trimmed now is trimmed next launch.
        }
    }
}
