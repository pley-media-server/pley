using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

namespace Pley.Tv.Core;

/// <summary>
/// A value screens follow (the web TV app's Preact signals): set on the UI thread, and every
/// listener hears each change there.
/// </summary>
public sealed class Signal<T>
{
    T value;

    public Signal(T initial) => value = initial;

    public event Action<T>? Changed;

    public T Value
    {
        get => value;
        set
        {
            if (EqualityComparer<T>.Default.Equals(this.value, value)) return;
            this.value = value;
            Changed?.Invoke(value);
        }
    }

    /// <summary>Calls <paramref name="listener"/> now and on every change; disposing stops it.</summary>
    public IDisposable Watch(Action<T> listener)
    {
        listener(value);
        Changed += listener;
        return new Unwatch(() => Changed -= listener);
    }

    sealed class Unwatch : IDisposable
    {
        Action? undo;

        public Unwatch(Action undo) => this.undo = undo;

        public void Dispose()
        {
            undo?.Invoke();
            undo = null;
        }
    }
}

/// <summary>Time and timers: the UI thread's on the TV, a clock moved by hand in tests.</summary>
public interface IScheduler
{
    /// <summary>Milliseconds since the epoch: for times that are kept, never for how long something took.</summary>
    long Now { get; }

    /// <summary>
    /// Milliseconds on a clock that only moves forward (the system's clock can be set back or
    /// ahead): for how long something took or has been held.
    /// </summary>
    long Elapsed { get; }

    /// <summary>Runs <paramref name="action"/> on the UI thread after <paramref name="ms"/>; disposing cancels it.</summary>
    IDisposable After(long ms, Action action);

    /// <summary>Runs <paramref name="action"/> on the UI thread soon, after what was posted before it.</summary>
    void Post(Action action);
}

/// <summary>Timers on the synchronization context of the thread that makes it: the UI thread's, on the TV.</summary>
public sealed class ContextScheduler : IScheduler
{
    readonly SynchronizationContext? context = SynchronizationContext.Current;
    readonly System.Diagnostics.Stopwatch clock = System.Diagnostics.Stopwatch.StartNew();

    public long Now => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

    public long Elapsed => clock.ElapsedMilliseconds;

    public IDisposable After(long ms, Action action)
    {
        var cancel = new CancellationTokenSource();
        Task.Delay(TimeSpan.FromMilliseconds(Math.Max(0, ms)), cancel.Token).ContinueWith(t =>
        {
            if (t.IsCanceled) return;
            if (context != null) context.Post(_ => { if (!cancel.IsCancellationRequested) action(); }, null);
            else action();
        }, TaskScheduler.Default);
        return new Cancel(cancel);
    }

    /// <summary>Cancels the timer: disposing a CancellationTokenSource doesn't, so it can't be the handle.</summary>
    sealed class Cancel : IDisposable
    {
        readonly CancellationTokenSource source;

        public Cancel(CancellationTokenSource source) => this.source = source;

        public void Dispose() => source.Cancel();
    }

    public void Post(Action action)
    {
        if (context != null) context.Post(_ => action(), null);
        else action();
    }
}
