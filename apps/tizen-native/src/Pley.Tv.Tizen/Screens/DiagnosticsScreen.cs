using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Diagnostics (the Android app's TvDiagnosticsScreen), opened from Settings: "Diagnostics", then
/// "This TV" with a line per fact (the label 180 dp wide, then the value), then "What it plays"
/// with a line each, 2 dp apart from x = 96 to the safe edge. Every line takes focus (the accent
/// fill, 36 dp at least, 16 dp in), so the remote scrolls through them; nothing acts but Check
/// again, after a check that failed.
/// </summary>
sealed class DiagnosticsScreen : ShellPage
{
    const float Width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal, LabelWidth = 180, Inset = 16, Gap = 2, Bottom = 160;

    readonly DiagnosticsModel model;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View content = new View();
    /// <summary>How far the page is scrolled, in pixels.</summary>
    float scroll;
    bool gone;

    public DiagnosticsScreen(TvShell shell, DiagnosticsModel model) : base(shell, new DiagnosticsPage(), "diagnostics")
    {
        this.model = model;
        View.BackgroundColor = PleyColors.Bg;
        content.PositionX = Ui.Dp(PleyTv.Gutter);
        View.Add(content);
        Group.Exit = d => d == Direction.Left && Shell.FocusRail();
        watches.Add(model.State.Watch(Draw));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    public override void Gone()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
    }

    void Draw(DiagnosticsState state)
    {
        if (gone) return;
        var current = Tree.Current;
        var focused = current != null && current.Parent == Group ? current.Name : null;
        foreach (var child in content.Children.ToList())
        {
            content.Remove(child);
            child.Dispose();
        }
        Group.Clear();

        var y = Ui.Dp(PleyTv.SafeVertical);
        void Place(View view, float height)
        {
            view.PositionY = y;
            content.Add(view);
            y += height + Ui.Dp(Gap);
        }
        void Heading(string text, TextStyle style, float above, float below)
        {
            y += Ui.Dp(above);
            var label = Text.Label(text, style, PleyColors.Fg, width: Width);
            label.PositionY = y;
            content.Add(label);
            y += label.SizeHeight + Ui.Dp(below);
        }
        void Line(string key, string? label, string value)
        {
            var line = new LineView(key, label, value);
            var top = y;
            var height = line.SizeHeight;
            var first = Group.Children.Count == 0;
            line.Item.FocusChanged += on =>
            {
                if (on && !gone) Reveal(top, height, first);
            };
            Group.Add(line.Item);
            Place(line, height);
        }

        Heading(DiagnosticsText.Title, PleyType.PageTitle, 0, 20);
        Heading(DiagnosticsText.Device, PleyType.Section, 0, 8);
        y += Ui.Dp(Gap);
        for (var i = 0; i < state.Device.Count; i++)
            Line($"device-{i}", state.Device[i].Label, state.Device[i].Value);
        Heading(DiagnosticsText.Plays, PleyType.Section, 20, 8);
        y += Ui.Dp(Gap);
        if (state.Lines.Count > 0)
        {
            for (var i = 0; i < state.Lines.Count; i++)
                Line($"line-{i}", null, state.Lines[i]);
        }
        else if (state.Probing)
        {
            var probing = Text.Label(DiagnosticsText.Probing, PleyType.Body, PleyColors.Muted, width: Width - Inset);
            probing.PositionX = Ui.Dp(Inset);
            Place(probing, probing.SizeHeight);
        }
        else
        {
            var failed = Text.Label(DiagnosticsText.ProbeFailed, PleyType.Body, PleyColors.DangerText, width: Width - Inset);
            failed.PositionX = Ui.Dp(Inset);
            Place(failed, failed.SizeHeight + Ui.Dp(12 - Gap));
            var again = new ButtonView(DiagnosticsText.ProbeAgain, ButtonKind.Pill, model.Probe);
            var top = y;
            again.Item.FocusChanged += on =>
            {
                if (on && !gone) Reveal(top, Ui.Dp(PleyTv.ButtonHeight), false);
            };
            Group.Add(again.Item);
            Place(again, Ui.Dp(PleyTv.ButtonHeight));
        }
        content.SizeHeight = y + Ui.Dp(Bottom);
        content.PositionY = -scroll;

        if (focused == null) return;
        // Focus stays on its line; Check again gone (checking), the first line has it.
        var same = Group.Children.OfType<FocusItem>().FirstOrDefault(c => c.Name == focused);
        if (same == null || !Tree.Focus(same)) Tree.Enter(Group);
    }

    /// <summary>Keeps the focused line in view, inside the safe area; the page's top for its first line, so the title shows.</summary>
    void Reveal(float top, float height, bool first)
    {
        var start = Ui.Dp(PleyTv.SafeVertical);
        var end = Ui.Dp(PleyTv.CanvasHeight - PleyTv.SafeVertical);
        if (first) scroll = 0;
        else if (top - scroll < start) scroll = top - start;
        else if (top + height - scroll > end) scroll = top + height - end;
        content.PositionY = -scroll;
    }

    /// <summary>A line of facts: the label (Body, muted, 180 dp) and the value on as many lines as it takes; focused, the accent fill.</summary>
    sealed class LineView : View
    {
        readonly TextLabel? label;
        readonly TextLabel value;

        public LineView(string key, string? label, string text)
        {
            Item = new FocusItem(key);
            CornerRadius = Ui.Dp(PleyShape.FieldRadius);
            var x = Inset;
            if (label != null)
            {
                this.label = Text.Label(label, PleyType.Body, PleyColors.Muted, width: LabelWidth);
                this.label.Position = new Position(Ui.Dp(x), Ui.Dp(8));
                Add(this.label);
                x += LabelWidth;
            }
            value = SettingsScreen.Lines(text, PleyType.Body, PleyColors.Fg, Width - x - Inset, 6);
            value.Position = new Position(Ui.Dp(x), Ui.Dp(8));
            Add(value);
            Size = new Size(Ui.Dp(Width), Math.Max(Ui.Dp(36), value.SizeHeight + Ui.Dp(16)));
            Item.FocusChanged += Look;
            Look(false);
        }

        public FocusItem Item { get; }

        void Look(bool focused)
        {
            if (Disposed) return;
            BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
            if (label != null) label.TextColor = focused ? PleyColors.OnAccent : PleyColors.Muted;
            value.TextColor = focused ? PleyColors.OnAccent : PleyColors.Fg;
        }
    }
}
