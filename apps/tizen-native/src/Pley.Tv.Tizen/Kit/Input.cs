using System;
using System.Collections.Generic;
using Pley.Tv.Core.Ui;
using Tizen.NUI;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The remote's keys: through the key-repeat gate, to the focused item's own keys, then to whatever
/// is on top (a dialog, a page), then the D-pad rules. OK selects as it comes up, unless it was held
/// for a long press, which opens the item's actions once (D-pad rule 6), as Android's long click.
/// </summary>
sealed class Input
{
    /// <summary>How long OK is held before it counts as a long press (Android's long-press timeout).</summary>
    public const long LongPressMs = 500;

    readonly Func<FocusTree?> tree;
    readonly KeyRepeatGate gate;
    readonly List<Func<string, bool, bool>> handlers = new List<Func<string, bool, bool>>();
    /// <summary>While a key is held, focus moves on this clock, about once a frame, not on the remote's uneven repeats.</summary>
    readonly Timer hold = new Timer(16);
    long okDownAt = -1;
    bool okLong;

    public Input(Window window, Func<FocusTree?> tree, KeyRepeatGate gate)
    {
        this.tree = tree;
        this.gate = gate;
        window.KeyEvent += (s, e) => OnKey(e.Key.KeyPressedName, e.Key.State == Key.StateType.Down);
        hold.Tick += (s, e) =>
        {
            var direction = gate.Tick();
            if (direction != null) Dispatch(direction, true);
            return gate.Down;
        };
    }

    /// <summary>Back: closes what is on top or goes back a page; the shell sets it.</summary>
    public Action? Back { get; set; }

    /// <summary>
    /// A handler for keys before the D-pad rules (a dialog, the player): true when it took the key.
    /// The newest goes first; disposing removes it.
    /// </summary>
    public IDisposable Push(Func<string, bool, bool> handler)
    {
        handlers.Add(handler);
        return new Undo(() => handlers.Remove(handler));
    }

    static long Now => System.Diagnostics.Stopwatch.GetTimestamp() * 1000 / System.Diagnostics.Stopwatch.Frequency;

    void OnKey(string key, bool down)
    {
        var admitted = gate.Admit(key, down);
        // A direction is down: if it stays down, the hold's clock moves focus from 250 ms on.
        if (gate.Down && !hold.IsRunning()) hold.Start();
        if (admitted) Dispatch(key, down);
    }

    void Dispatch(string key, bool down)
    {
        var focus = tree();
        var item = focus?.Current;
        if (item?.OwnKey?.Invoke(key, down) == true) return;
        for (var i = handlers.Count - 1; i >= 0; i--)
            if (handlers[i](key, down)) return;
        switch (key)
        {
            case "Up":
                if (down) Move(focus, Direction.Up);
                break;
            case "Down":
                if (down) Move(focus, Direction.Down);
                break;
            case "Left":
                if (down) Move(focus, Direction.Left);
                break;
            case "Right":
                if (down) Move(focus, Direction.Right);
                break;
            case "Return":
            case "KP_Enter":
                Ok(item, down);
                break;
            case "XF86Back":
            case "Escape":
                if (down) Back?.Invoke();
                break;
            // The remote's Menu key (Samsung's Tools) opens the focused item's actions as it comes up.
            case "XF86SysMenu":
            case "XF86Menu":
            case "Menu":
                if (!down) item?.Menu?.Invoke();
                break;
        }
    }

    static void Move(FocusTree? focus, Direction direction) => focus?.Move(direction);

    void Ok(FocusItem? item, bool down)
    {
        if (down)
        {
            if (okDownAt < 0)
            {
                okDownAt = Now;
                okLong = false;
            }
            else if (!okLong && item?.LongSelected != null && Now - okDownAt >= LongPressMs)
            {
                // Held: the actions, once however long it stays down.
                okLong = true;
                item.LongSelected();
            }
            return;
        }
        var wasLong = okLong;
        okDownAt = -1;
        okLong = false;
        if (!wasLong) item?.Selected?.Invoke();
    }

    sealed class Undo : IDisposable
    {
        Action? undo;

        public Undo(Action undo) => this.undo = undo;

        public void Dispose()
        {
            undo?.Invoke();
            undo = null;
        }
    }
}
