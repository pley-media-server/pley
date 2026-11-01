using System;
using System.Threading;

namespace Pley.Tv.Tizen.Kit;

/// <summary>What a media key on the remote asks for.</summary>
enum MediaAction
{
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
    Rewind,
    Forward,
    Stop,
}

/// <summary>
/// The remote's media keys: a Samsung remote's come through the window (XF86Audio*) or through the
/// TV's media key service, and some TVs send both for one press, so the same key twice within
/// 250 ms counts once. A key goes to <c>act</c> only while <c>wanted</c> says so; otherwise it is
/// left to whatever else listens.
/// </summary>
sealed class MediaKeys : IDisposable
{
    /// <summary>The same media key from the window and from the media key service this soon after counts once.</summary>
    const long TwiceMs = 250;

    readonly Func<bool> wanted;
    readonly Action<MediaAction> act;
    readonly IDisposable keys;
    readonly SynchronizationContext? ui = SynchronizationContext.Current;
    readonly bool service;
    MediaAction? last;
    long lastAt;
    bool disposed;

    public MediaKeys(Input input, Func<bool> wanted, Action<MediaAction> act)
    {
        this.wanted = wanted;
        this.act = act;
        keys = input.Push(OnKey);
        try
        {
            global::Tizen.System.MediaKey.Event += OnService;
            service = true;
        }
        catch (Exception)
        {
            // Not on this TV: the window's keys only.
        }
    }

    /// <summary>The action of a window key's name; null for any other key.</summary>
    public static MediaAction? Of(string key) => key switch
    {
        "XF86AudioPlay" => MediaAction.Play,
        "XF86AudioPause" => MediaAction.Pause,
        "XF86AudioPlayPause" => MediaAction.PlayPause,
        "XF86AudioNext" => MediaAction.Next,
        "XF86AudioPrev" => MediaAction.Previous,
        "XF86AudioRewind" => MediaAction.Rewind,
        "XF86AudioForward" => MediaAction.Forward,
        "XF86AudioStop" => MediaAction.Stop,
        _ => null,
    };

    bool OnKey(string key, bool down)
    {
        if (disposed || Of(key) is not { } action || !wanted()) return false;
        if (down) Fire(action);
        return true;
    }

    void OnService(object? sender, global::Tizen.System.MediaKeyEventArgs e)
    {
        if (e.Status != global::Tizen.System.MediaKeyEventArgs.KeyStatus.Pressed) return;
        MediaAction? action = e.Value switch
        {
            global::Tizen.System.MediaKeyEventArgs.KeyValue.Play => MediaAction.Play,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.Pause => MediaAction.Pause,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.PlayPause => MediaAction.PlayPause,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.Next => MediaAction.Next,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.Previous => MediaAction.Previous,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.Rewind => MediaAction.Rewind,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.FastForward => MediaAction.Forward,
            global::Tizen.System.MediaKeyEventArgs.KeyValue.Stop => MediaAction.Stop,
            _ => null,
        };
        if (action is not { } a) return;
        if (ui == null) FromService(a);
        else ui.Post(_ => FromService(a), null);
    }

    void FromService(MediaAction action)
    {
        if (!disposed && wanted()) Fire(action);
    }

    void Fire(MediaAction action)
    {
        var now = Environment.TickCount64;
        if (last == action && now - lastAt < TwiceMs) return;
        last = action;
        lastAt = now;
        act(action);
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        keys.Dispose();
        if (!service) return;
        try
        {
            global::Tizen.System.MediaKey.Event -= OnService;
        }
        catch (Exception)
        {
            // It never took.
        }
    }
}
