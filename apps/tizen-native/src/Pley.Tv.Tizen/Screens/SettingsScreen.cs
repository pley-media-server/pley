using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The TV's settings (design spec §4.8; the Android app's TvSettingsScreen), in two panes under
/// the title: the categories on the left (256 dp at x = 96), the focused category's rows on the
/// right (x = 376 to 912), each pane's rows 2 dp apart. Focus on a category shows its rows; Right
/// or OK goes into them, at their first. A choice opens a side panel, and focus comes back to its
/// row; a switch toggles on OK. "Manage your account on the web" shows the address and a QR code,
/// since a TV opens no browser, and Sign out asks first. What the page tells ("Saved") shows at
/// the top right for 3.5 s.
/// </summary>
sealed class SettingsScreen : ShellPage
{
    /// <summary>Where the panes start: under the title and 20 dp.</summary>
    const float Top = PleyTv.SafeVertical + 34 + 20, Gap = 2, PaneWidth = SettingsModel.PaneRight - SettingsModel.PaneX;

    readonly Shell layers;
    readonly SettingsModel model;
    readonly Images images;
    readonly Action<PageKey> go;
    readonly Action<PageKey> restart;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly FocusGroup categories = new FocusGroup(Axis.Column, "categories");
    readonly FocusGroup pane = new FocusGroup(Axis.Column, "pane");
    readonly Dictionary<SettingsCategory, SettingRowView> categoryRows = new Dictionary<SettingsCategory, SettingRowView>();
    readonly View rows = new View();
    readonly View message;
    readonly TextLabel messageText;
    readonly Timer messageTimer = new Timer((uint)SettingsModel.MessageMs);
    SettingsCategory shown = SettingsCategory.Profile;
    /// <summary>How far the pane is scrolled, in pixels.</summary>
    float scroll;
    bool gone;

    /// <param name="go">Opens a page over this one: Watch history, Libraries, Diagnostics.</param>
    /// <param name="restart">Starts over at a page: Who's watching?, the servers.</param>
    public SettingsScreen(TvShell shell, Shell layers, SettingsModel model, Images images, Action<PageKey> go, Action<PageKey> restart)
        : base(shell, new PageKey.Settings(), "settings")
    {
        this.layers = layers;
        this.model = model;
        this.images = images;
        this.go = go;
        this.restart = restart;
        View.BackgroundColor = PleyColors.Bg;
        var title = Text.Label(SettingsText.Title, PleyType.PageTitle, PleyColors.Fg, width: 480);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(title);

        var columns = new FocusGroup(Axis.Row, "settings");
        columns.Add(categories);
        columns.Add(pane);
        Group.Add(columns);
        categories.Exit = d => d == Direction.Left && Shell.FocusRail();

        var y = Top;
        foreach (var c in SettingsModel.Categories)
        {
            var category = c;
            var row = new SettingRowView("category-" + c, SettingsText.CategoryLabel(c), SettingsModel.CategoriesWidth, icon: SettingsText.CategoryIcon(c),
                danger: c == SettingsCategory.SignOut, chevronRoom: true);
            row.Position = new Position(Ui.Dp(SettingsModel.CategoriesX), Ui.Dp(y));
            y += row.HeightPx / Ui.Scale + Gap;
            row.Item.FocusChanged += on =>
            {
                if (!on || gone || shown == category) return;
                shown = category;
                scroll = 0;
                DrawPane();
            };
            row.Item.Selected = () => Select(category);
            // Right goes into the category's rows at their first, whatever is beside it.
            row.Item.OwnKey = (key, down) => key == "Right" && down && FocusFirst(shown);
            categoryRows[c] = row;
            categories.Add(row.Item);
            View.Add(row);
        }

        var viewport = new View
        {
            Position = new Position(Ui.Dp(SettingsModel.PaneX), Ui.Dp(Top)),
            Size = new Size(Ui.Dp(PaneWidth), Ui.Dp(PleyTv.CanvasHeight - Top)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        viewport.Add(rows);
        View.Add(viewport);

        message = new View { CornerRadius = Ui.Dp(PleyShape.ControlRadius), BackgroundColor = PleyColors.Surface3 };
        messageText = Text.Label("", PleyType.Label, PleyColors.Fg);
        messageText.Position = new Position(Ui.Dp(20), Ui.Dp(10));
        message.Add(messageText);
        message.Hide();
        View.Add(message);
        messageTimer.Tick += (s, e) =>
        {
            if (!gone) message.Hide();
            return false;
        };

        model.Told += Tell;
        // Signed out: the app starts over at signing in, as it does whenever the sign-in ends (AppState.SignedOut).
        watches.Add(model.State.Watch(_ => DrawPane()));
    }

    public override void Gone()
    {
        gone = true;
        foreach (var w in watches) w.Dispose();
        model.Told -= Tell;
        messageTimer.Stop();
        model.Dispose();
        model.Me.Dispose();
    }

    void Tell(string text)
    {
        if (gone) return;
        messageText.Text = text;
        var width = messageText.NaturalSize.Width;
        messageText.Size = new Size(width + 2, Text.LineHeight(PleyType.Label));
        message.Size = new Size(width + Ui.Dp(40), messageText.SizeHeight + Ui.Dp(20));
        message.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal) - message.SizeWidth, Ui.Dp(PleyTv.SafeVertical));
        message.Show();
        messageTimer.Stop();
        messageTimer.Start();
    }

    /// <summary>OK on a category: Sign out asks; the others go into their rows.</summary>
    void Select(SettingsCategory c)
    {
        if (c == SettingsCategory.SignOut)
        {
            AskSignOut();
            return;
        }
        if (shown != c)
        {
            shown = c;
            scroll = 0;
            DrawPane();
        }
        FocusFirst(c);
    }

    bool FocusFirst(SettingsCategory c) => FocusRow(model.FirstRow(c));

    bool FocusRow(string key)
    {
        foreach (var node in pane.Children)
            if (node is FocusItem item && item.Name == key) return Tree.Focus(item);
        return false;
    }

    /// <summary>Draws the shown category's pane again; focus in it stays on its row, by key.</summary>
    void DrawPane()
    {
        if (gone) return;
        foreach (var c in categoryRows)
            c.Value.Chevron = c.Key == shown;
        var current = Tree.Current;
        var focused = current != null && current.Parent == pane ? current.Name : null;
        foreach (var child in rows.Children.ToList())
        {
            rows.Remove(child);
            child.Dispose();
        }
        pane.Clear();

        float y = 0;
        FocusItem? firstRow = null;
        foreach (var item in model.Items(shown))
        {
            View view;
            float height;
            switch (item)
            {
                case SettingsItem.Row r:
                    var row = new SettingRowView(r.RowKey, r.Label, PaneWidth, r.Value, r.Icon, r.Checked, r.Chevron, r.Danger, r.Enabled);
                    row.Item.Selected = () => Act(r.Action);
                    var top = y;
                    var rowHeight = row.HeightPx;
                    var first = firstRow == null && r.Enabled;
                    firstRow ??= r.Enabled ? row.Item : null;
                    row.Item.FocusChanged += on =>
                    {
                        if (on && !gone) Reveal(top, rowHeight, first);
                    };
                    pane.Add(row.Item);
                    view = row;
                    height = rowHeight;
                    break;
                case SettingsItem.ProfileHeader h:
                    view = ProfileHeader(h.Profile);
                    height = Ui.Dp(60);
                    break;
                case SettingsItem.Heading h:
                    var heading = Text.Label(h.Text, PleyType.Label, PleyColors.Subtle, width: PaneWidth - 20);
                    heading.Position = new Position(Ui.Dp(20), Ui.Dp(16));
                    view = new View { Size = new Size(Ui.Dp(PaneWidth), Ui.Dp(16 + 6) + heading.SizeHeight) };
                    view.Add(heading);
                    height = view.SizeHeight;
                    break;
                case SettingsItem.Intro intro:
                    var words = Lines(intro.Text, PleyType.Body, PleyColors.Muted, PaneWidth - 20, 4);
                    words.PositionX = Ui.Dp(20);
                    view = new View { Size = new Size(Ui.Dp(PaneWidth), words.SizeHeight + Ui.Dp(8)) };
                    view.Add(words);
                    height = view.SizeHeight;
                    break;
                case SettingsItem.Facts facts:
                    view = Facts(facts.List);
                    height = view.SizeHeight;
                    break;
                default:
                    continue;
            }
            view.PositionY = y;
            rows.Add(view);
            y += height + Ui.Dp(Gap);
        }
        rows.PositionY = -scroll;

        if (focused == null) return;
        if (!FocusRow(focused) && !FocusFirst(shown)) Tree.Focus(categoryRows[shown].Item);
    }

    /// <summary>Keeps the focused row in view: the pane's top for its first row, so what is above it shows.</summary>
    void Reveal(float top, float height, bool first)
    {
        var room = Ui.Dp(PleyTv.CanvasHeight - Top - PleyTv.SafeVertical);
        if (first || top < scroll) scroll = first ? 0 : top;
        else if (top + height > scroll + room) scroll = top + height - room;
        rows.PositionY = -scroll;
    }

    void Act(SettingAction action)
    {
        switch (action)
        {
            case SettingAction.Pick pick:
                Choose(pick.Choice);
                break;
            case SettingAction.Toggle toggle:
                model.Toggle(toggle.Switch);
                break;
            case SettingAction.Open open:
                go(open.Key);
                break;
            case SettingAction.Restart again:
                restart(again.Key);
                break;
            case SettingAction.Web web:
                new WebDialog(layers, model.State.Value.Me.Profile?.Server ?? "", web.Path).Show();
                break;
            case SettingAction.SignOut:
                AskSignOut();
                break;
        }
    }

    /// <summary>The choice's side panel; choosing makes it, and focus comes back to the row that opened it.</summary>
    void Choose(SettingPick pick)
    {
        var panel = model.Panel(pick);
        var lines = panel.Choices.Select(c => new SidePanelSheet.Line(c.Label, Checked: c.Checked, Hint: c.Hint)).ToList();
        var sheet = new SidePanelSheet(layers, panel.Title, lines, panel.Focus, i =>
        {
            model.Choose(pick, i);
            return false;
        });
        sheet.Gone += () =>
        {
            if (!gone) FocusRow(pick.RowKey);
        };
        sheet.Show();
    }

    /// <summary>"Sign out?", Cancel focused; focus goes back to the Sign out category once it closes.</summary>
    void AskSignOut()
    {
        var server = model.State.Value.Me.Profile?.Server ?? "";
        var dialog = new Dialog(layers, SettingsText.SignOutTitle, SettingsText.SignOutBody(server));
        dialog.Button(SettingsText.Cancel, dialog.Close, initial: true);
        dialog.Button(SettingsText.SignOut, () =>
        {
            dialog.Close();
            model.SignOut();
        }, PleyColors.DangerText);
        dialog.Gone += () =>
        {
            if (!gone) Tree.Focus(categoryRows[SettingsCategory.SignOut].Item);
        };
        dialog.Show();
    }

    /// <summary>Who is signed in: the avatar (48 dp), the name in Title and its line in Caption; 60 dp of room while it isn't known.</summary>
    View ProfileHeader(MeProfile? profile)
    {
        var view = new View { Size = new Size(Ui.Dp(PaneWidth), Ui.Dp(60)) };
        if (profile == null) return view;
        var avatar = new AvatarView(profile.Name, profile.AvatarColor, profile.AvatarUrl, 48, images) { Position = new Position(Ui.Dp(20), 0) };
        view.Add(avatar);
        const float x = 20 + 48 + 16;
        var name = Text.Label(profile.Name, PleyType.Title, PleyColors.Fg, width: PaneWidth - x);
        var line = Text.Label(profile.Line, PleyType.Caption, PleyColors.Muted, width: PaneWidth - x);
        var top = (Ui.Dp(48) - name.SizeHeight - line.SizeHeight) / 2;
        name.Position = new Position(Ui.Dp(x), top);
        line.Position = new Position(Ui.Dp(x), top + name.SizeHeight);
        view.Add(name);
        view.Add(line);
        return view;
    }

    /// <summary>The TV's facts under "Open diagnostics": label (Caption, muted, 150 dp) and value, 6 dp apart, 20 dp in and 12 below the row.</summary>
    static View Facts(IReadOnlyList<Fact> facts)
    {
        var view = new View();
        var line = Text.LineHeight(PleyType.Caption);
        var y = Ui.Dp(12);
        for (var i = 0; i < facts.Count; i++)
        {
            if (i > 0) y += Ui.Dp(6);
            var label = Text.Label(facts[i].Label, PleyType.Caption, PleyColors.Muted, width: 150);
            label.Position = new Position(Ui.Dp(20), y);
            var value = Text.Label(facts[i].Value, PleyType.Caption, PleyColors.Fg, width: PaneWidth - 20 - 150);
            value.Position = new Position(Ui.Dp(20 + 150), y);
            view.Add(label);
            view.Add(value);
            y += line;
        }
        view.Size = new Size(Ui.Dp(PaneWidth), y);
        return view;
    }

    /// <summary>Text on as many lines as it takes at <paramref name="widthDp"/>, <paramref name="max"/> at most, set from the top.</summary>
    internal static TextLabel Lines(string text, TextStyle style, Color color, float widthDp, int max)
    {
        var label = Text.Label(text, style, color, maxLines: max, width: widthDp);
        label.VerticalAlignment = VerticalAlignment.Top;
        var line = Text.LineHeight(style);
        label.SizeHeight = Math.Min(Math.Max(label.GetHeightForWidth(Ui.Dp(widthDp)), line), line * max);
        return label;
    }
}

/// <summary>
/// A settings row (the Android app's TvSettingRow): 44 dp at least, 20 dp in, an optional 20 dp
/// icon, the label in Body and the value under it in Caption (two lines at most), muted; a switch
/// (40 × 22) at its end, or a chevron. Focused, the accent fill with OnAccent ink, never scaled.
/// Danger rows read in danger text; a disabled one is greyed and can't take focus.
/// </summary>
sealed class SettingRowView : View
{
    const float Inset = 20, SwitchWidth = 40, SwitchHeight = 22;

    readonly bool danger;
    readonly bool enabled;
    readonly bool? isOn;
    readonly TextLabel label;
    readonly TextLabel? value;
    readonly ImageView? icon;
    readonly ImageView? chevron;
    readonly View? track;
    readonly View? thumb;

    /// <param name="chevronRoom">Room for a chevron that comes and goes (the categories': the one shown has it).</param>
    public SettingRowView(string key, string text, float widthDp, string? value = null, string? icon = null, bool? isOn = null, bool chevron = false,
        bool danger = false, bool enabled = true, bool chevronRoom = false)
    {
        this.danger = danger;
        this.enabled = enabled;
        this.isOn = isOn;
        Item = new FocusItem(key) { Enabled = enabled };
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);

        var x = Inset;
        var right = widthDp - Inset;
        if (isOn != null)
        {
            track = new View { Size = new Size(Ui.Dp(SwitchWidth), Ui.Dp(SwitchHeight)), CornerRadius = Ui.Dp(SwitchHeight / 2) };
            const float knob = SwitchHeight - 6;
            thumb = new View
            {
                Size = new Size(Ui.Dp(knob), Ui.Dp(knob)),
                CornerRadius = Ui.Dp(knob / 2),
                Position = new Position(Ui.Dp(isOn.Value ? SwitchWidth - 3 - knob : 3), Ui.Dp(3)),
            };
            track.Add(thumb);
            Add(track);
            right -= SwitchWidth + 12;
        }
        else if (chevron || chevronRoom)
        {
            this.chevron = Icons.Icon("chevron-right", 20, PleyColors.Subtle);
            Add(this.chevron);
            if (!chevron) this.chevron.Hide();
            right -= 20 + 12;
        }
        if (icon != null)
        {
            this.icon = Icons.Icon(icon, PleyTv.IconSize, PleyColors.Muted);
            Add(this.icon);
            x += PleyTv.IconSize + 12;
        }
        var room = Math.Max(40, right - x);
        label = Text.Label(text, PleyType.Body, PleyColors.Fg, width: room);
        Add(label);
        var textHeight = label.SizeHeight;
        if (!string.IsNullOrEmpty(value))
        {
            this.value = SettingsScreen.Lines(value!, PleyType.Caption, PleyColors.Muted, room, 2);
            Add(this.value);
            textHeight += this.value.SizeHeight;
        }
        HeightPx = Math.Max(Ui.Dp(PleyTv.ListRowHeight), textHeight + Ui.Dp(16));
        Size = new Size(Ui.Dp(widthDp), HeightPx);
        var textTop = (HeightPx - textHeight) / 2;
        label.Position = new Position(Ui.Dp(x), textTop);
        if (this.value != null) this.value.Position = new Position(Ui.Dp(x), textTop + label.SizeHeight);
        if (this.icon != null) this.icon.Position = new Position(Ui.Dp(Inset), (HeightPx - Ui.Dp(PleyTv.IconSize)) / 2);
        if (this.chevron != null) this.chevron.Position = new Position(Ui.Dp(widthDp - Inset - 20), (HeightPx - Ui.Dp(20)) / 2);
        if (track != null) track.Position = new Position(Ui.Dp(widthDp - Inset - SwitchWidth), (HeightPx - Ui.Dp(SwitchHeight)) / 2);
        Item.FocusChanged += Look;
        Look(false);
    }

    public FocusItem Item { get; }

    /// <summary>The row's height in pixels.</summary>
    public float HeightPx { get; }

    /// <summary>Shows or hides the chevron (a category: the one whose rows show).</summary>
    public bool Chevron
    {
        set
        {
            if (chevron == null || Disposed) return;
            if (value) chevron.Show();
            else chevron.Hide();
        }
    }

    void Look(bool focused)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
        label.TextColor = focused ? PleyColors.OnAccent : danger ? PleyColors.DangerText : enabled ? PleyColors.Fg : PleyColors.Disabled;
        if (value != null) value.TextColor = focused ? new Color(PleyColors.OnAccent.R, PleyColors.OnAccent.G, PleyColors.OnAccent.B, 0.8f) : enabled ? PleyColors.Muted : PleyColors.Disabled;
        if (icon != null) icon.Color = focused ? PleyColors.OnAccent : danger ? PleyColors.DangerText : PleyColors.Muted;
        if (chevron != null) chevron.Color = focused ? PleyColors.OnAccent : PleyColors.Subtle;
        if (track != null && thumb != null)
        {
            var on = isOn == true;
            track.BackgroundColor = focused
                ? on ? PleyColors.OnAccent : new Color(PleyColors.OnAccent.R, PleyColors.OnAccent.G, PleyColors.OnAccent.B, 0.3f)
                : on ? PleyColors.Accent : PleyColors.Surface4;
            thumb.BackgroundColor = focused
                ? on ? PleyColors.Accent : PleyColors.OnAccent
                : on ? PleyColors.OnAccent : PleyColors.Control;
        }
    }
}
