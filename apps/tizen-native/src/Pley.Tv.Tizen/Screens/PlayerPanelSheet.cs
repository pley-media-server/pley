using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The player's side panels (design spec §2.7; the Android app's TvPlayerPanels): Subtitles (with
/// Sync while the subtitles can move, and their size, background, font and position), Audio,
/// Quality, Chapters, Sleep timer, Speed and Info, as <see cref="PlayerModel.Panel"/> lists them.
/// 360 dp (Info 440 dp) from the right edge, full height, Surface2 with rounded left corners, over
/// the scrim; it slides in. Focus starts on the current choice and stays inside; choosing closes
/// the panel, but for the subtitles' look and Sync, which change what shows behind it, and Back
/// closes it too. Sync's Left and Right move the subtitles, OK puts them back. Info reads its
/// numbers once a second while it shows; Up and Down scroll it.
/// </summary>
sealed class PlayerPanelSheet : Sheet
{
    const float Inset = 24, InfoStep = 96;

    readonly Shell shell;
    readonly PlayerModel model;
    readonly PlayerPanelKind kind;
    readonly float width;
    readonly float inner;
    readonly View card;
    readonly View list = new View();
    readonly Timer? reader;
    float scroll;
    float bottom;
    bool gone;

    public PlayerPanelSheet(Shell shell, PlayerModel model, PlayerPanelKind kind) : base("player-panel")
    {
        this.shell = shell;
        this.model = model;
        this.kind = kind;
        var panel = model.Panel(kind);
        width = panel.Wide ? PlayerPanel.InfoWidth : PlayerPanel.Width;
        inner = width - Inset - PleyTv.SafeHorizontal;
        View.BackgroundColor = PleyColors.Scrim;
        card = new View
        {
            Size = new Size(Ui.Dp(width), Ui.Dp(PleyTv.CanvasHeight)),
            Position = new Position(Ui.Dp(PleyTv.CanvasWidth - width), 0),
            BackgroundColor = PleyColors.Surface2,
            CornerRadius = new Vector4(Ui.Dp(PleyShape.DialogRadius), 0, 0, Ui.Dp(PleyShape.DialogRadius)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        // Level 2's hairline along the top.
        card.Add(new View { Size = new Size(Ui.Dp(width), Ui.Dp(1)), BackgroundColor = PleyColors.Surface4 });
        var viewport = new View
        {
            Position = new Position(Ui.Dp(Inset), Ui.Dp(PleyTv.SafeVertical)),
            Size = new Size(Ui.Dp(inner), Ui.Dp(Room)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        viewport.Add(list);
        card.Add(viewport);
        View.Add(card);
        Fill(panel, panel.Focus);
        if (kind == PlayerPanelKind.Info)
        {
            // The details are asked for while Info shows; its numbers read again each second.
            model.SetDetailsOpen(true);
            reader = new Timer((uint)Readout.ReadMs);
            reader.Tick += (_, _) =>
            {
                if (!gone) Fill(model.Panel(kind), -1);
                return !gone;
            };
            reader.Start();
        }
        Gone += () =>
        {
            gone = true;
            reader?.Stop();
            if (kind == PlayerPanelKind.Info) model.SetDetailsOpen(false);
        };
    }

    /// <summary>The content's height in dp, between the safe area's top and bottom.</summary>
    static float Room => PleyTv.CanvasHeight - 2 * PleyTv.SafeVertical;

    /// <summary>Puts the panel over the player, sliding in from the right edge.</summary>
    public void Show()
    {
        shell.Push(this);
        card.PositionX = Ui.Dp(PleyTv.CanvasWidth);
        var slide = new Animation(PleyMotion.OverlayMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        slide.AnimateTo(card, "PositionX", Ui.Dp(PleyTv.CanvasWidth - width));
        slide.Play();
    }

    public void Close()
    {
        if (!gone) shell.Pop(this);
    }

    /// <summary>Draws <paramref name="panel"/>, focus on the row at <paramref name="focus"/> (none: where the panel scrolls).</summary>
    void Fill(PlayerPanel panel, int focus)
    {
        foreach (var child in list.Children.ToList())
        {
            list.Remove(child);
            child.Dispose();
        }
        Root.Clear();
        bottom = 0;
        var heading = PanelColumn.Lines(panel.Title, PleyType.Title, PleyColors.Fg, inner - 16, 2);
        Place(heading, heading.SizeHeight / Ui.Scale, 16, 8);
        bottom += 12;
        FocusItem? first = null;
        for (var i = 0; i < panel.Rows.Count; i++)
        {
            var row = panel.Rows[i];
            var at = i;
            switch (row.Kind)
            {
                case PlayerRowKind.Heading:
                    Place(Text.Label(row.Label, PleyType.Label, PleyColors.Muted, width: inner - 16), PleyType.Label.Line, 16, 16);
                    bottom += 6;
                    break;
                case PlayerRowKind.Note:
                    var note = PanelColumn.Lines(row.Label, PleyType.Body, PleyColors.Muted, inner - 32, 4);
                    Place(note, note.SizeHeight / Ui.Scale, 16, 8);
                    bottom += 8;
                    break;
                case PlayerRowKind.Fact:
                    Fact(row);
                    break;
                case PlayerRowKind.Step:
                    var step = new StepRow(row, inner, d => Step(row, d, at), () => Choose(row, at));
                    var stepItem = Item(step, step.SizeHeight / Ui.Scale, step.Item);
                    if (at == focus) first = stepItem;
                    break;
                default:
                    var choice = new PanelChoice(row.Label, () => Choose(row, at), row.Icon, row.Supporting, row.Checked, width: inner);
                    var choiceItem = Item(choice, choice.SizeHeight / Ui.Scale, choice.Item);
                    if (at == focus || (first == null && at > focus && focus >= 0)) first = choiceItem;
                    break;
            }
        }
        bottom += 24;
        if (Root.Children.Count == 0)
        {
            // Nothing to choose (Info): focus holds the panel, and Up and Down scroll it.
            var reading = new FocusItem("readout")
            {
                OwnKey = (key, down) =>
                {
                    if (key != "Up" && key != "Down") return false;
                    if (down) ScrollTo(scroll + (key == "Down" ? InfoStep : -InfoStep));
                    return true;
                },
            };
            Root.Add(reading);
            first = reading;
        }
        ScrollTo(scroll);
        Tree.Focus(first ?? (FocusItem)Root.Children[0]);
    }

    /// <summary>Places <paramref name="view"/> under what is there, <paramref name="height"/> dp tall.</summary>
    float Place(View view, float height, float x, float gap)
    {
        var top = bottom + gap;
        view.Position = new Position(Ui.Dp(x), Ui.Dp(top));
        list.Add(view);
        bottom = top + height;
        return top;
    }

    /// <summary>A row focus can land on, which scrolls into view when it does.</summary>
    FocusItem Item(View view, float height, FocusItem item)
    {
        var top = Place(view, height, 0, 0);
        bottom += 4;
        Root.Add(item);
        item.FocusChanged += on =>
        {
            if (!on || gone) return;
            if (top < scroll) ScrollTo(top);
            else if (top + height > scroll + Room) ScrollTo(top + height - Room);
            // The first rows sit under the title: back near the top, the title shows again.
            if (top <= Room - height && scroll <= top) ScrollTo(0);
        };
        return item;
    }

    /// <summary>A line of the readout: its label, muted in a 112 dp column, and its value beside it.</summary>
    void Fact(PlayerRow row)
    {
        const float x = 16, gap = 12;
        var valueWidth = inner - 2 * x - PlayerPanel.InfoLabelWidth - gap;
        var label = PanelColumn.Lines(row.Label, PleyType.Caption, PleyColors.Muted, PlayerPanel.InfoLabelWidth, 2);
        var value = PanelColumn.Lines(row.Value ?? "", PleyType.Caption, PleyColors.Fg, valueWidth, 3);
        var height = Math.Max(label.SizeHeight, value.SizeHeight) / Ui.Scale;
        var line = new View { Size = new Size(Ui.Dp(inner - 2 * x), Ui.Dp(height)) };
        line.Add(label);
        value.PositionX = Ui.Dp(PlayerPanel.InfoLabelWidth + gap);
        line.Add(value);
        Place(line, height, x, 4);
    }

    void ScrollTo(float to)
    {
        scroll = Math.Max(0, Math.Min(Math.Max(0, bottom - Room), to));
        list.PositionY = -Ui.Dp(scroll);
    }

    /// <summary>OK on a row: the model does what it says, and the panel closes or shows the change.</summary>
    void Choose(PlayerRow row, int index)
    {
        if (gone) return;
        if (!model.Choose(row)) Close();
        else Fill(model.Panel(kind), index);
    }

    void Step(PlayerRow row, int direction, int index)
    {
        if (gone) return;
        model.Step(row, direction);
        Fill(model.Panel(kind), index);
    }

    /// <summary>
    /// A row Left and Right step and OK resets (Sync): its label and what it does on the left, the
    /// value between chevrons on the right; focused, an accent pill.
    /// </summary>
    sealed class StepRow : View
    {
        const float Pad = 16, Glyph = 20;

        readonly TextLabel label;
        readonly TextLabel hint;
        readonly TextLabel value;
        readonly ImageView left;
        readonly ImageView right;

        public StepRow(PlayerRow row, float width, Action<int> step, Action reset)
        {
            Item = new FocusItem(row.Label)
            {
                Selected = reset,
                OwnKey = (key, down) =>
                {
                    if (key != "Left" && key != "Right") return false;
                    if (down) step(key == "Left" ? -1 : 1);
                    return true;
                },
            };
            value = Text.Label(row.Value ?? "", PleyType.Label, PleyColors.Fg);
            var valueWidth = Text.Width(value);
            var textWidth = width - 2 * Pad - Glyph - 12 - valueWidth - 12 - Glyph;
            label = Text.Label(row.Label, PleyType.Label, PleyColors.Fg, width: textWidth);
            hint = PanelColumn.Lines(row.Supporting ?? "", PleyType.Caption, PleyColors.Muted, textWidth, 2);
            var height = Math.Max(PleyTv.ListRowHeight, 8 + PleyType.Label.Line + hint.SizeHeight / Ui.Scale + 8);
            Size = new Size(Ui.Dp(width), Ui.Dp(height));
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            var textTop = (height - PleyType.Label.Line - hint.SizeHeight / Ui.Scale) / 2;
            label.Position = new Position(Ui.Dp(Pad), Ui.Dp(textTop));
            hint.Position = new Position(Ui.Dp(Pad), Ui.Dp(textTop + PleyType.Label.Line));
            Add(label);
            Add(hint);
            var x = width - Pad - Glyph;
            right = Icons.Icon("chevron-right", Glyph, PleyColors.Muted);
            right.Position = new Position(Ui.Dp(x), Ui.Dp((height - Glyph) / 2));
            Add(right);
            x -= 6 + valueWidth;
            value.Size = new Size(Ui.Dp(valueWidth), Ui.Dp(height));
            value.Position = new Position(Ui.Dp(x), 0);
            Add(value);
            x -= 6 + Glyph;
            left = Icons.Icon("chevron-left", Glyph, PleyColors.Muted);
            left.Position = new Position(Ui.Dp(x), Ui.Dp((height - Glyph) / 2));
            Add(left);
            Item.FocusChanged += Look;
        }

        public FocusItem Item { get; }

        void Look(bool focused)
        {
            if (Disposed) return;
            BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
            var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
            var muted = focused ? PleyColors.OnAccent : PleyColors.Muted;
            label.TextColor = ink;
            value.TextColor = ink;
            hint.TextColor = muted;
            left.Color = muted;
            right.Color = muted;
        }
    }
}
