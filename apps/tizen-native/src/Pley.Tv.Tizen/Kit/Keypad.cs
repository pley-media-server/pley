using System;
using System.Collections.Generic;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>A key of a TV pad: a digit, "." or ":" (the address pad), Delete or OK.</summary>
abstract record PadKey
{
    public sealed record Digit(int Value) : PadKey;
    public sealed record Symbol(string Text) : PadKey;
    public sealed record Delete : PadKey;
    public sealed record Ok : PadKey;

    /// <summary>The PIN pad's keys (design spec §2.9): 1-9, then ⌫, 0, OK.</summary>
    public static readonly PadKey[][] PinKeys =
    {
        new PadKey[] { new Digit(1), new Digit(2), new Digit(3) },
        new PadKey[] { new Digit(4), new Digit(5), new Digit(6) },
        new PadKey[] { new Digit(7), new Digit(8), new Digit(9) },
        new PadKey[] { new Delete(), new Digit(0), new Ok() },
    };

    /// <summary>The address pad's keys (design spec §2.11): 1-9, then ".", 0, ":". Delete and Connect sit under it.</summary>
    public static readonly PadKey[][] AddressKeys =
    {
        new PadKey[] { new Digit(1), new Digit(2), new Digit(3) },
        new PadKey[] { new Digit(4), new Digit(5), new Digit(6) },
        new PadKey[] { new Digit(7), new Digit(8), new Digit(9) },
        new PadKey[] { new Symbol("."), new Digit(0), new Symbol(":") },
    };
}

/// <summary>
/// One round key of a pad (the Android app's TvPadKey): 64 dp, its corners a control's; Surface3
/// with PinDigit, focused Accent at 1.08. A press flashes it Surface4 for 100 ms.
/// </summary>
sealed class PadKeyView : View
{
    public const float KeySize = 64, Gap = 14;

    readonly TextLabel? glyph;
    readonly ImageView? icon;
    readonly Timer flashTimer = new Timer(PleyMotion.PressMs);
    bool flashing;
    Animation? focusAnimation;

    public PadKeyView(PadKey key, Action<PadKey> onKey)
    {
        Key = key;
        Size = new Size(Ui.Dp(KeySize), Ui.Dp(KeySize));
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        PivotPoint = global::Tizen.NUI.PivotPoint.Center;
        Item = new FocusItem(key.ToString())
        {
            Selected = () =>
            {
                Flash();
                onKey(key);
            },
        };
        switch (key)
        {
            case PadKey.Digit d:
                glyph = Glyph(d.Value.ToString());
                break;
            case PadKey.Symbol s:
                glyph = Glyph(s.Text);
                break;
            default:
                icon = Icons.Icon(key is PadKey.Delete ? "delete" : "check", 28, PleyColors.Fg);
                icon.Position = new Position(Ui.Dp((KeySize - 28) / 2), Ui.Dp((KeySize - 28) / 2));
                Add(icon);
                break;
        }
        flashTimer.Tick += (s, e) =>
        {
            flashing = false;
            Look();
            return false;
        };
        Item.FocusChanged += focused =>
        {
            if (Disposed) return;
            Look();
            focusAnimation?.Stop();
            var scale = focused ? PleyShape.RoundScale : 1f;
            focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(this, "Scale", new Vector3(scale, scale, 1));
            focusAnimation.Play();
        };
        Look();
    }

    public PadKey Key { get; }
    public FocusItem Item { get; }

    /// <summary>Shows the key pressed: by OK on it, or by the remote's digit keys.</summary>
    public void Flash()
    {
        flashing = true;
        Look();
        flashTimer.Stop();
        flashTimer.Start();
    }

    TextLabel Glyph(string text)
    {
        var label = Text.Label(text, PleyType.PinDigit, PleyColors.Fg, width: KeySize);
        label.SizeHeight = Ui.Dp(KeySize);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        Add(label);
        return label;
    }

    void Look()
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        BackgroundColor = flashing ? PleyColors.Surface4 : Item.Focused ? PleyColors.Accent : PleyColors.Surface3;
        var ink = Item.Focused && !flashing ? PleyColors.OnAccent : PleyColors.Fg;
        if (glyph != null) glyph.TextColor = ink;
        if (icon != null) icon.Color = ink;
    }

    protected override void Dispose(global::Tizen.NUI.DisposeTypes type)
    {
        if (Disposed) return;
        flashTimer.Stop();
        flashTimer.Dispose();
        base.Dispose(type);
    }
}

/// <summary>A pad's keys, 3 × 4, 14 apart, as a grid that focus moves through; focus starts on 5.</summary>
sealed class Keypad : View
{
    readonly List<PadKeyView> keys = new List<PadKeyView>();

    public Keypad(PadKey[][] rows, Action<PadKey> onKey)
    {
        Group = new FocusGroup(Axis.Grid, "keypad") { Columns = rows[0].Length };
        for (var r = 0; r < rows.Length; r++)
        {
            for (var c = 0; c < rows[r].Length; c++)
            {
                var key = new PadKeyView(rows[r][c], onKey)
                {
                    Position = new Position(Ui.Dp(c * (PadKeyView.KeySize + PadKeyView.Gap)), Ui.Dp(r * (PadKeyView.KeySize + PadKeyView.Gap))),
                };
                keys.Add(key);
                Add(key);
                Group.Add(key.Item);
            }
        }
        var columns = rows[0].Length;
        Size = new Size(Ui.Dp(columns * PadKeyView.KeySize + (columns - 1) * PadKeyView.Gap), Ui.Dp(rows.Length * PadKeyView.KeySize + (rows.Length - 1) * PadKeyView.Gap));
    }

    public FocusGroup Group { get; }

    /// <summary>The key where focus starts: 5.</summary>
    public FocusItem Initial => keys.Find(k => k.Key == new PadKey.Digit(5))?.Item ?? keys[0].Item;

    /// <summary>The OK key, where a pad has one.</summary>
    public FocusItem? Ok => keys.Find(k => k.Key is PadKey.Ok)?.Item;

    /// <summary>Flashes <paramref name="key"/>, pressed on the remote rather than on screen.</summary>
    public void Flash(PadKey key) => keys.Find(k => k.Key == key)?.Flash();
}
