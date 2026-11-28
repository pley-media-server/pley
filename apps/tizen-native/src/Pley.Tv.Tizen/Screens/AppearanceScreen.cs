using System.Collections.Generic;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Settings → Appearance (docs/THEMES.md, "In every app"; the web's ThemeCard): the nine themes in
/// a grid of five, each a small Pley in its colors with its name and caption, then Default and the
/// six accents, each with its swatch in the theme shown. The pick shown is outlined in the accent
/// and checked; focus is the TV's ring on a theme, the accent fill on an accent. OK shows a pick at
/// once and keeps it on the TV for the profile (AppState.Theme): the app draws every page again in it, this one
/// with focus where it was.
/// </summary>
sealed class AppearanceScreen : ShellPage
{
    const float Width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
    const float Gap = 14, RowGap = 16, Pad = 6, Columns = Appearance.Columns;
    const float CardWidth = (Width - (Columns - 1) * Gap) / Columns, PreviewWidth = CardWidth - 2 * Pad, PreviewHeight = PreviewWidth * 9 / 16;
    const float CheckSize = 22, CheckInset = 5;
    const float HeadingY = PleyTv.SafeVertical + 34 + 10, GridY = HeadingY + 30, PillHeight = PleyTv.ButtonHeight;

    readonly AppState state;
    readonly bool atAccents;
    readonly FocusGroup themes = new FocusGroup(Axis.Grid, "themes") { Columns = Appearance.Columns };
    readonly FocusGroup accents = new FocusGroup(Axis.Row, "accents");
    FocusItem? checkedTheme, checkedAccent;
    bool entered;

    /// <param name="atAccents">Opened from Settings' Accent row: focus starts on the accent shown.</param>
    public AppearanceScreen(TvShell shell, AppState state, bool atAccents) : base(shell, new AppearancePage(atAccents), "appearance")
    {
        this.state = state;
        this.atAccents = atAccents;
        View.BackgroundColor = PleyColors.Bg;
        var title = Text.Label(ThemeText.Title, PleyType.PageTitle, PleyColors.Fg, width: Width);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(title);
        Group.Add(themes);
        Group.Add(accents);
        Group.Exit = d => d == Direction.Left && Shell.FocusRail();

        var pick = state.Theme.Value;
        Heading(ThemeText.Theme, ThemeText.ThemeHint, HeadingY);
        var choices = Appearance.Themes(pick);
        var cardHeight = 0f;
        for (var i = 0; i < choices.Count; i++)
        {
            var choice = choices[i];
            var card = new ThemeCard(choice, () => Shell.ReduceMotion);
            cardHeight = card.HeightDp;
            card.Position = new Position(Ui.Dp(PleyTv.Gutter + i % Appearance.Columns * (CardWidth + Gap)), Ui.Dp(GridY + i / Appearance.Columns * (card.HeightDp + RowGap)));
            card.Item.Selected = () =>
            {
                if (state.Theme.Value.Theme != choice.Id) state.ChooseTheme(choice.Id);
            };
            themes.Add(card.Item);
            View.Add(card);
            if (!choice.Checked) continue;
            checkedTheme = card.Item;
            // Up from the accents lands on the theme shown.
            themes.RememberChild(i);
        }

        var rows = (choices.Count + Appearance.Columns - 1) / Appearance.Columns;
        var accentY = GridY + rows * cardHeight + (rows - 1) * RowGap + 18;
        Heading(ThemeText.Accent, ThemeText.AccentHint, accentY);
        var x = PleyTv.Gutter;
        foreach (var choice in Appearance.Accents(pick))
        {
            var id = choice.Id;
            var pill = new AccentPill(choice, () => Shell.ReduceMotion);
            pill.Position = new Position(Ui.Dp(x), Ui.Dp(accentY + 30));
            x += pill.WidthDp + PleyTv.ButtonGap;
            pill.Item.Selected = () =>
            {
                if (state.Theme.Value.Accent != id) state.ChooseAccent(id);
            };
            accents.Add(pill.Item);
            View.Add(pill);
            if (!choice.Checked) continue;
            checkedAccent = pill.Item;
            // Down from the themes lands on the accent shown.
            accents.RememberChild(accents.Children.Count - 1);
        }
    }

    /// <summary>A section's name in Label and its hint after it on the same line, in Caption, muted.</summary>
    void Heading(string name, string hint, float y)
    {
        var label = Text.Label(name, PleyType.Label, PleyColors.Fg);
        var width = Text.Width(label);
        label.Size = new Size(Ui.Dp(width), Text.LineHeight(PleyType.Label));
        label.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        View.Add(label);
        var words = Text.Label(hint, PleyType.Caption, PleyColors.Muted, width: Width - width - 12);
        words.Position = new Position(Ui.Dp(PleyTv.Gutter + width + 12), Ui.Dp(y) + (label.SizeHeight - words.SizeHeight) / 2);
        View.Add(words);
    }

    /// <summary>Opened, focus is on the pick shown, the theme or the accent as the row that opened it; back on top, where it was.</summary>
    public override void Enter()
    {
        if (!entered)
        {
            entered = true;
            if ((atAccents ? checkedAccent : checkedTheme) is { } item && Tree.Focus(item)) return;
        }
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    /// <summary>Where focus is: among the accents or the themes, and which.</summary>
    public (bool Accents, int Index)? Spot
    {
        get
        {
            var current = Tree.Current;
            if (current == null) return null;
            var list = current.Parent == accents ? accents : current.Parent == themes ? themes : null;
            if (list == null) return null;
            for (var i = 0; i < list.Children.Count; i++)
                if (list.Children[i] == current) return (list == accents, i);
            return null;
        }
    }

    /// <summary>Focus back at <paramref name="spot"/>, as it was before the page was drawn again (in the theme just picked).</summary>
    public void Return((bool Accents, int Index) spot)
    {
        entered = true;
        var list = spot.Accents ? accents : themes;
        if (spot.Index < list.Children.Count && list.Children[spot.Index] is FocusItem item) Tree.Focus(item);
    }

    /// <summary>
    /// A theme: a small Pley in its colors, its name in Label and its caption in Caption under it
    /// (two lines). The one shown has the accent's edge and a check; focused, the TV's ring in place of its edge.
    /// </summary>
    sealed class ThemeCard : View
    {
        readonly View ring;
        /// <summary>The card's own edge: the accent on the theme shown, Surface4 on the others.</summary>
        readonly Color edge;
        readonly System.Func<bool> reduceMotion;
        Animation? focusAnimation;

        public ThemeCard(ThemeChoice choice, System.Func<bool> reduceMotion)
        {
            this.reduceMotion = reduceMotion;
            Item = new FocusItem("theme-" + choice.Id);
            var nameHeight = Text.LineHeight(PleyType.Label);
            var aboutHeight = Text.LineHeight(PleyType.Caption) * 2;
            HeightDp = Pad + PreviewHeight + 8 + (nameHeight + aboutHeight) / Ui.Scale + Pad;
            Size = new Size(Ui.Dp(CardWidth), Ui.Dp(HeightDp));
            CornerRadius = Ui.Dp(PleyShape.CardRadius + 2);
            PivotPoint = global::Tizen.NUI.PivotPoint.Center;
            BorderlineWidth = Ui.Dp(choice.Checked ? 2 : 1);
            BorderlineOffset = -1;
            edge = choice.Checked ? PleyColors.Accent : PleyColors.Surface4;
            BorderlineColor = edge;
            BackgroundColor = choice.Checked ? PleyColors.Alpha(PleyColors.Accent, 0.06f) : Color.Transparent;

            var preview = Preview(ThemeColors.Of(choice.Palette));
            preview.Position = new Position(Ui.Dp(Pad), Ui.Dp(Pad));
            Add(preview);

            var name = Text.Label(choice.Name, PleyType.Label, PleyColors.Fg, width: PreviewWidth - 2);
            name.Position = new Position(Ui.Dp(Pad + 2), Ui.Dp(Pad + PreviewHeight + 8));
            Add(name);
            var about = SettingsScreen.Lines(choice.About, PleyType.Caption, PleyColors.Muted, PreviewWidth - 2, 2);
            about.Position = new Position(Ui.Dp(Pad + 2), name.PositionY + nameHeight);
            Add(about);

            // The TV's focus ring, as a tile's: 3 dp, 4.5 dp clear of the card.
            var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
            ring = new View
            {
                Size = new Size(SizeWidth + 2 * reach, SizeHeight + 2 * reach),
                Position = new Position(-reach, -reach),
                CornerRadius = CornerRadius + reach,
                BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
                BorderlineColor = PleyColors.AccentBright,
                BorderlineOffset = 0,
            };
            ring.Hide();
            Add(ring);
            // At the preview's bottom right, 5 dp in from both edges, as on every TV app: at the top right
            // it sat against the preview's accent button. Added last, so it stays over the card and the ring.
            if (choice.Checked) Add(Check(Pad + PreviewWidth - CheckInset - CheckSize, Pad + PreviewHeight - CheckInset - CheckSize));
            Item.FocusChanged += Look;
        }

        public FocusItem Item { get; }

        public float HeightDp { get; }

        void Look(bool focused)
        {
            // Let go already: a focus change can still reach it on its way to the view in its place.
            if (Disposed) return;
            // One outline: while the ring shows, the card's own edge goes, which beside it drew a second line.
            BorderlineColor = focused ? Color.Transparent : edge;
            if (focused) ring.Show();
            else ring.Hide();
            focusAnimation?.Stop();
            var scale = focused ? PleyShape.PillScale : 1f;
            if (reduceMotion())
            {
                Scale = new Vector3(scale, scale, 1);
                return;
            }
            focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(this, "Scale", new Vector3(scale, scale, 1));
            focusAnimation.Play();
        }

        /// <summary>
        /// The web's preview: the sidebar with the accent's bar and three faint ones, then a heading
        /// with a button, a muted line and a row of four posters, the first ringed in the accent.
        /// </summary>
        static View Preview(ThemeColors c)
        {
            const float w = PreviewWidth, h = PreviewHeight, p = w * 0.05f, bar = h * 0.07f, side = w / 4;
            var preview = new View
            {
                Size = new Size(Ui.Dp(w), Ui.Dp(h)),
                CornerRadius = Ui.Dp(PleyShape.CardRadius),
                BackgroundColor = c.Bg,
                ClippingMode = ClippingModeType.ClipChildren,
                BorderlineWidth = Ui.Dp(1),
                BorderlineColor = PleyColors.Alpha(c.Fg, 0.1f),
                BorderlineOffset = -1,
            };
            preview.Add(Box(0, 0, side, h, c.Surface, 0));
            preview.Add(Box(side - 0.5f, 0, 0.5f, h, c.Surface4, 0));
            var faint = PleyColors.Alpha(c.Fg, 0.15f);
            var widths = new[] { 0.75f, 1f, 0.66f, 0.83f };
            for (var i = 0; i < widths.Length; i++)
                preview.Add(Box(p, p + i * bar * 2.2f, (side - 2 * p) * widths[i], bar, i == 0 ? c.Accent : faint, bar / 2));
            var left = side + p;
            var main = w - left - p;
            preview.Add(Box(left, p, main * 0.5f, bar * 1.2f, PleyColors.Alpha(c.Fg, 0.8f), bar * 0.6f));
            preview.Add(Box(left + main * 0.75f, p - bar * 0.15f, main * 0.25f, bar * 1.5f, c.Accent, bar * 0.75f));
            preview.Add(Box(left, p + bar * 2.2f, main * 0.33f, bar, PleyColors.Alpha(c.Muted, 0.5f), bar / 2));
            const float gap = 3;
            var poster = (main - 3 * gap) / 4;
            var top = h - p - poster * 1.5f;
            for (var i = 0; i < 4; i++)
            {
                var box = Box(left + i * (poster + gap), top, poster, poster * 1.5f, c.Surface3, 1.5f);
                if (i == 0)
                {
                    box.BorderlineWidth = Ui.Dp(1);
                    box.BorderlineColor = c.Accent;
                    box.BorderlineOffset = -1;
                }
                preview.Add(box);
            }
            return preview;
        }

        static View Box(float x, float y, float w, float h, Color color, float radius) => new View
        {
            Position = new Position(Ui.Dp(x), Ui.Dp(y)),
            Size = new Size(Ui.Dp(w), Ui.Dp(h)),
            BackgroundColor = color,
            CornerRadius = Ui.Dp(radius),
        };
    }

    /// <summary>The pick's check: the accent's circle with a check in its words' color, ringed in the page's color.</summary>
    static View Check(float x, float y)
    {
        var badge = new View
        {
            Position = new Position(Ui.Dp(x), Ui.Dp(y)),
            Size = new Size(Ui.Dp(CheckSize), Ui.Dp(CheckSize)),
            CornerRadius = Ui.Dp(CheckSize / 2),
            BackgroundColor = PleyColors.Accent,
            BorderlineWidth = Ui.Dp(2),
            BorderlineColor = PleyColors.Bg,
            BorderlineOffset = -1,
        };
        var check = Icons.Icon("check", 12, PleyColors.OnAccent);
        check.Position = new Position(Ui.Dp(5), Ui.Dp(5));
        badge.Add(check);
        return badge;
    }

    /// <summary>
    /// An accent: its swatch (20 dp, in the theme shown) and its name, 40 tall. The one shown has the
    /// accent's edge, its swatch checked; focused, the accent fill with its words' color, as a button.
    /// </summary>
    sealed class AccentPill : View
    {
        readonly bool picked;
        readonly TextLabel label;
        readonly View swatch;
        readonly System.Func<bool> reduceMotion;
        Animation? focusAnimation;

        public AccentPill(AccentChoice choice, System.Func<bool> reduceMotion)
        {
            this.reduceMotion = reduceMotion;
            picked = choice.Checked;
            Item = new FocusItem("accent-" + (choice.Id ?? "own"));
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            PivotPoint = global::Tizen.NUI.PivotPoint.Center;
            BorderlineWidth = Ui.Dp(1);
            BorderlineOffset = -1;
            var colors = ThemeColors.Of(choice.Palette);
            swatch = new View
            {
                Size = new Size(Ui.Dp(20), Ui.Dp(20)),
                Position = new Position(Ui.Dp(12), Ui.Dp((PillHeight - 20) / 2)),
                CornerRadius = Ui.Dp(10),
                BackgroundColor = colors.Accent,
                BorderlineWidth = Ui.Dp(1),
                BorderlineOffset = -1,
            };
            if (picked)
            {
                var check = Icons.Icon("check", 12, colors.OnAccent);
                check.Position = new Position(Ui.Dp(4), Ui.Dp(4));
                swatch.Add(check);
            }
            Add(swatch);
            label = Text.Label(choice.Name, PleyType.Label, PleyColors.Fg);
            var textWidth = Text.Width(label);
            label.Size = new Size(Ui.Dp(textWidth), Ui.Dp(PillHeight));
            label.Position = new Position(Ui.Dp(12 + 20 + 8), 0);
            Add(label);
            WidthDp = 12 + 20 + 8 + textWidth + 14;
            Size = new Size(Ui.Dp(WidthDp), Ui.Dp(PillHeight));
            Item.FocusChanged += Look;
            Look(false);
        }

        public FocusItem Item { get; }

        public float WidthDp { get; }

        void Look(bool focused)
        {
            // Let go already: a focus change can still reach it on its way to the view in its place.
            if (Disposed) return;
            BackgroundColor = focused ? PleyColors.Accent : picked ? PleyColors.Alpha(PleyColors.Accent, 0.06f) : Color.Transparent;
            BorderlineColor = focused ? Color.Transparent : picked ? PleyColors.Accent : PleyColors.Surface4;
            label.TextColor = focused ? PleyColors.OnAccent : picked ? PleyColors.Fg : PleyColors.Muted;
            // On the accent's own fill, the swatch of the same color shows by its edge.
            swatch.BorderlineColor = focused ? PleyColors.OnAccent : PleyColors.Alpha(PleyColors.Fg, 0.2f);
            focusAnimation?.Stop();
            var scale = focused ? PleyShape.PillScale : 1f;
            if (reduceMotion())
            {
                Scale = new Vector3(scale, scale, 1);
                return;
            }
            focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(this, "Scale", new Vector3(scale, scale, 1));
            focusAnimation.Play();
        }
    }
}
