using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>What Live TV's pages and its player draw alike (the Android app's TvLiveKit).</summary>
static class LiveViews
{
    /// <summary>A programme airing in the guide, the New badge: the Android app's PrimaryContainer.</summary>
    public static readonly Color PrimaryContainer = new Color(0x14 / 255f, 0x30 / 255f, 0x3E / 255f, 1);

    /// <summary>The icon of Core's Lucide name, among those the app ships (packages/tv's set): All channels has layers there.</summary>
    public static string Icon(string name) => name switch
    {
        "layout-grid" => "layers",
        "triangle-alert" => "circle-alert",
        _ => name,
    };

    public static Color Alpha(Color c, float alpha) => new Color(c.R, c.G, c.B, alpha);

    /// <summary>How far <paramref name="program"/> is at <paramref name="now"/>: a track with the accent over it, 3 dp tall.</summary>
    public static View Progress(LiveProgram program, long now, float widthDp, Color track, float heightDp = 3)
    {
        var bar = new View
        {
            Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp)),
            BackgroundColor = track,
            CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
        };
        // No clip (a GPU pass a bar): the filled part rounds its own corners.
        bar.Add(new View { Size = new Size(Ui.Dp(widthDp * program.ProgressAt(now)), Ui.Dp(heightDp)), BackgroundColor = PleyColors.Accent, CornerRadius = Ui.Dp(PleyShape.BadgeRadius) });
        return bar;
    }

    /// <summary>A small boxed word in capitals: Live in danger red, New in the accent.</summary>
    public static View Badge(string text, bool live)
    {
        var label = Text.Label(text.ToUpperInvariant(), PleyType.Badge, live ? PleyColors.Bg : PleyColors.AccentBright);
        var width = label.NaturalSize.Width + Ui.Dp(10);
        var box = new View
        {
            Size = new Size(width, Ui.Dp(PleyType.Badge.Line + 4)),
            BackgroundColor = live ? PleyColors.Danger : PrimaryContainer,
            CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
        };
        label.Size = new Size(width, box.SizeHeight);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        box.Add(label);
        return box;
    }

    /// <summary>The badge a programme carries, if any: Live first, else New.</summary>
    public static View? BadgeOf(LiveProgram program) => LiveText.Badge(program) is { } text ? Badge(text, program.IsLive) : null;

    /// <summary>A ring in the accent, turning while a channel tunes or waits for data.</summary>
    public static (View View, Animation Spin) Spinner(float sizeDp)
    {
        var arc = new ArcVisual { Thickness = Ui.Dp(3), StartAngle = 0, SweepAngle = 270, Cap = ArcVisual.CapType.Round, MixColor = PleyColors.Accent };
        var view = new View
        {
            Size = new Size(Ui.Dp(sizeDp), Ui.Dp(sizeDp)),
            Background = arc.OutputVisualMap,
            PivotPoint = global::Tizen.NUI.PivotPoint.Center,
            PositionUsesPivotPoint = false,
        };
        var spin = new Animation(900) { Looping = true, DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.Linear) };
        spin.AnimateBy(view, "Orientation", new Rotation(new Radian(new Degree(360)), PositionAxis.Z));
        return (view, spin);
    }

    /// <summary>A label of <paramref name="style"/> sized to its text, for a line of parts.</summary>
    public static TextLabel Fitted(string text, TextStyle style, Color color, float maxWidthDp)
    {
        var label = Text.Label(Text.Fit(text, style, maxWidthDp), style, color);
        var width = Math.Min(Text.Width(label), maxWidthDp);
        label.Size = new Size(Ui.Dp(width), Text.LineHeight(style));
        return label;
    }
}

/// <summary>
/// A channel's logo in a rounded well, kept to its shape and inset 8 dp; without one (or until it
/// comes), the channel's number on its own gradient, or its name when asked (the Android app's
/// ChannelTile).
/// </summary>
sealed class ChannelLogoView : View
{
    readonly TextLabel label;
    readonly Color well;
    readonly float inset;

    /// <param name="cancel">Gives up the logo while it comes: the view's row went.</param>
    public ChannelLogoView(LiveChannel channel, string origin, Images images, float widthDp, float heightDp, bool name = false, TextStyle? style = null,
        Color? well = null, float radiusDp = PleyShape.TileRadius, float insetDp = 8, CancellationToken cancel = default)
    {
        this.well = well ?? PleyColors.Surface3;
        inset = insetDp;
        Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp));
        CornerRadius = Ui.Dp(radiusDp);
        // No clip (a GPU pass a logo, dozens in the guide): the logo and its name sit inside the well.
        var text = LiveText.LogoLabel(channel, name);
        var (from, to) = Placeholder.Colors(text);
        Background = Gradients.Diagonal(AvatarView.Rgb(from), AvatarView.Rgb(to));
        var s = style ?? PleyType.Label;
        var lines = name && heightDp >= s.Line * 2 + 12 ? 2 : 1;
        label = Text.Label(lines == 1 ? Text.Fit(text, s, widthDp - 8) : text, s, LiveViews.Alpha(PleyColors.Fg, 0.85f), maxLines: lines);
        label.Size = new Size(Ui.Dp(widthDp - 8), Ui.Dp(heightDp - 4));
        label.Position = new Position(Ui.Dp(4), Ui.Dp(2));
        label.HorizontalAlignment = HorizontalAlignment.Center;
        Add(label);
        if (string.IsNullOrEmpty(channel.Logo)) return;
        images.Load(new[] { Artwork.Absolute(channel.Logo!, origin) }, widthDp, file =>
        {
            if (file == null || Disposed) return;
            BackgroundColor = this.well;
            label.Hide();
            var picture = new ImageView
            {
                Size = new Size(SizeWidth - Ui.Dp(2 * inset), SizeHeight - Ui.Dp(2 * inset)),
                Position = new Position(Ui.Dp(inset), Ui.Dp(inset)),
                Image = new ImageVisual
                {
                    URL = file,
                    VisualFittingMode = VisualFittingModeType.FitKeepAspectRatio,
                    DesiredWidth = (int)(SizeWidth - Ui.Dp(2 * inset)),
                    DesiredHeight = (int)(SizeHeight - Ui.Dp(2 * inset)),
                    FittingMode = FittingModeType.ShrinkToFit,
                }.OutputVisualMap,
            };
            Add(picture);
            // Under what was put on the well while it loaded (a card's number, its star).
            picture.LowerToBottom();
        }, cancel);
    }
}

/// <summary>
/// The categories as a column (the Android app's CategoryColumn): beside the guide under the
/// search, and in the live player's channel list. Rows are <see cref="LiveCategories"/>'s: 40 dp
/// tall and 2 apart, the category shown filled with Surface3 and its icon in the accent, the
/// focused one filled with the accent; headings in capitals. A region opens and closes with OK,
/// one at a time; a category's OK says it was <see cref="Chosen"/>. The focused row stays in view.
/// Every row has its place and focus, but only those near the screen have views: an IPTV
/// provider's 200 rows took 260 ms to make, each time Live TV opened or a channel was left.
/// </summary>
sealed class CategoryColumnView : View
{
    const float HeadingHeight = 33, Gap = 2, SearchRoom = TextInput.Height + 8;

    readonly FocusTree tree;
    readonly float width, height;
    readonly TextInput? search;
    readonly View content = new View();
    /// <summary>Every row: its place, its focus (none for a heading) and how its view is made (none for the search field).</summary>
    readonly List<(CategoryRow Row, FocusItem? Item, float Top, Func<View>? Make)> laid = new List<(CategoryRow, FocusItem?, float, Func<View>?)>();
    /// <summary>The rows with views, by their place in <see cref="laid"/>.</summary>
    readonly Dictionary<int, View> made = new Dictionary<int, View>();
    LiveGroups? groups;
    LiveCategory selected = new LiveCategory.All();
    IReadOnlyList<LiveGroup>? found;
    string? open;
    string? openedFor = "\0";
    float scroll;

    /// <param name="search">The search field, first in the column; it stays the caller's.</param>
    public CategoryColumnView(FocusTree tree, float widthDp, float heightDp, TextInput? search = null)
    {
        this.tree = tree;
        width = widthDp;
        height = heightDp;
        this.search = search;
        Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp));
        ClippingMode = ClippingModeType.ClipChildren;
        Add(content);
        Group = new FocusGroup(Axis.Column, "categories");
        // The search field is first: focused, the column goes back to its top so all of it shows.
        if (search != null) search.Item.FocusChanged += on =>
        {
            if (on && !Disposed) Reveal(0);
        };
    }

    public FocusGroup Group { get; }

    /// <summary>A category's row was chosen.</summary>
    public Action<LiveCategory>? Chosen { get; set; }

    /// <summary>The rows for <paramref name="groups"/>, <paramref name="selected"/> marked; focus stays on the row it was on.</summary>
    public void Show(LiveGroups? groups, LiveCategory selected, IReadOnlyList<LiveGroup>? found = null)
    {
        Profile.Mark("live categories: show");
        this.groups = groups;
        this.selected = selected;
        this.found = found;
        // The region of the category shown opens with it, as the column first shows and when another is shown.
        var region = LiveCategories.RegionOf(selected, LiveRules.Sections(groups?.Groups ?? Array.Empty<LiveGroup>()));
        if (region != openedFor)
        {
            openedFor = region;
            open = region;
        }
        Build();
    }

    /// <summary>Focus to the category shown (its region's row while it is closed), brought into view.</summary>
    public bool FocusShown()
    {
        var sections = LiveRules.Sections(groups?.Groups ?? Array.Empty<LiveGroup>());
        var rows = laid.Select(l => l.Row).ToList();
        var at = LiveCategories.ShownIndex(rows, selected, sections);
        var item = at < laid.Count ? laid[at].Item : null;
        item ??= laid.Select(l => l.Item).FirstOrDefault(i => i != null);
        return item != null && tree.Focus(item);
    }

    /// <summary>Whether focus is in the column.</summary>
    public bool HoldsFocus => tree.Current != null && tree.Current.Parent == Group;

    void Build()
    {
        if (Disposed) return;
        var started = Profile.Start();
        var focused = HoldsFocus ? tree.Current!.Name : null;
        // The search field stays where it is: taken off the screen while typing, the TV closes its
        // keyboard, and every letter typed rebuilds the rows under it.
        foreach (var view in made.Values)
        {
            view.Unparent();
            view.Dispose();
        }
        made.Clear();
        Group.Clear();
        laid.Clear();
        var rows = LiveCategories.Rows(groups, selected, open, search != null, found);
        float y = 0;
        FocusItem? again = null;
        foreach (var row in rows)
        {
            switch (row)
            {
                case CategoryRow.Search:
                    if (search == null) break;
                    search.Position = new Position(Ui.Dp(LiveCategories.Inset - 10), Ui.Dp(y));
                    if (search.GetParent() != content) content.Add(search);
                    Group.Add(search.Item);
                    laid.Add((row, search.Item, y, null));
                    if (search.Item.Name == focused) again = search.Item;
                    y += SearchRoom + Gap;
                    break;
                case CategoryRow.Heading heading:
                    var headingTop = y;
                    laid.Add((row, null, y, () =>
                    {
                        var label = Text.Label(heading.Text, PleyType.Badge, PleyColors.Subtle, width: width - LiveCategories.Inset);
                        label.Position = new Position(Ui.Dp(LiveCategories.Inset), Ui.Dp(headingTop + 14));
                        return label;
                    }));
                    y += HeadingHeight + Gap;
                    break;
                case CategoryRow.Choice choice:
                    y = AddRow(row, choice.Icon != null ? LiveViews.Icon(choice.Icon) : null, choice.Name, choice.Count, choice.Selected, choice.Indent, () => Chosen?.Invoke(choice.Category), y, focused, ref again);
                    break;
                case CategoryRow.Region region:
                    var name = region.Name;
                    y = AddRow(row, region.Open ? "chevron-down" : "chevron-right", region.Name, region.Count, false, LiveCategories.Inset, () => Toggle(name), y, focused, ref again);
                    break;
            }
        }
        content.Size = new Size(Ui.Dp(width), Ui.Dp(y));
        Draw();
        if (again != null) tree.Focus(again);
        else if (focused != null) FocusShown();
        Profile.Took($"live categories: {laid.Count} rows laid, {made.Count} made", started);
    }

    float AddRow(CategoryRow row, string? icon, string name, int? count, bool isSelected, float indent, Action choose, float y, string? focused, ref FocusItem? again)
    {
        var item = new FocusItem(row.Key) { Selected = choose };
        var top = y;
        Group.Add(item);
        laid.Add((row, item, y, () =>
        {
            var view = new CategoryRowView(item, width, icon, name, count, isSelected, indent);
            view.PositionY = Ui.Dp(top);
            return view;
        }));
        item.FocusChanged += on =>
        {
            if (on && !Disposed) Reveal(top);
        };
        if (row.Key == focused) again = item;
        return y + LiveCategories.RowHeight + Gap;
    }

    /// <summary>The rows in the column and half a column either side have views; the others go.</summary>
    void Draw()
    {
        var from = scroll - height / 2;
        var to = scroll + height * 1.5f;
        for (var i = 0; i < laid.Count; i++)
        {
            var (_, _, top, make) = laid[i];
            var near = top + LiveCategories.RowHeight >= from && top <= to;
            if (made.TryGetValue(i, out var view))
            {
                if (near) continue;
                view.Unparent();
                view.Dispose();
                made.Remove(i);
            }
            else if (near && make != null)
            {
                view = make();
                content.Add(view);
                made[i] = view;
            }
        }
    }

    /// <summary>A region opens, closing the one open; open, it closes.</summary>
    void Toggle(string region)
    {
        open = open == region ? null : region;
        Build();
    }

    void Reveal(float top)
    {
        var room = height - PleyTv.SafeVertical;
        if (top < scroll)
        {
            scroll = Math.Max(0, top - LiveCategories.RowHeight);
            // Going up, scrolled less than the search field's room, its top would sit under the column's edge.
            if (search != null && scroll < SearchRoom + Gap) scroll = 0;
        }
        else if (top + LiveCategories.RowHeight > scroll + room) scroll = top + LiveCategories.RowHeight - room;
        Draw();
        var a = new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        a.AnimateTo(content, "PositionY", -Ui.Dp(scroll));
        a.PlayHeld();
    }

    /// <summary>One category or region: its icon, name and count; the one shown marked; focused, the accent fill.</summary>
    sealed class CategoryRowView : View
    {
        readonly bool selected;
        readonly FocusItem item;
        readonly ImageView? icon;
        readonly TextLabel label;
        readonly TextLabel? count;

        public CategoryRowView(FocusItem item, float width, string? iconName, string name, int? number, bool selected, float indent)
        {
            this.selected = selected;
            this.item = item;
            Size = new Size(Ui.Dp(width), Ui.Dp(LiveCategories.RowHeight));
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            var x = indent;
            if (iconName != null)
            {
                icon = Icons.Icon(iconName, 16, PleyColors.Muted);
                icon.Position = new Position(Ui.Dp(x), Ui.Dp((LiveCategories.RowHeight - 16) / 2));
                Add(icon);
                x += 16 + 10;
            }
            var right = width - 10;
            if (number != null)
            {
                // As wide as the number, as Android has it: the name takes the rest of the row.
                count = Text.Label(LiveText.Count(number.Value), PleyType.Caption, PleyColors.Muted);
                var countWidth = Text.Width(count);
                count.Size = new Size(Ui.Dp(countWidth), Ui.Dp(PleyType.Caption.Line));
                count.Position = new Position(Ui.Dp(right - countWidth), Ui.Dp((LiveCategories.RowHeight - PleyType.Caption.Line) / 2));
                Add(count);
                right -= countWidth + 8;
            }
            label = Text.Label(name, PleyType.Label, PleyColors.Muted, width: Math.Max(20, right - x));
            label.Position = new Position(Ui.Dp(x), Ui.Dp((LiveCategories.RowHeight - PleyType.Label.Line) / 2));
            Add(label);
            item.FocusChanged += Look;
            Look(item.Focused);
        }

        void Look(bool focused)
        {
            if (Disposed) return;
            BackgroundColor = focused ? PleyColors.Accent : selected ? PleyColors.Surface3 : Color.Transparent;
            var ink = focused ? PleyColors.OnAccent : selected ? PleyColors.Fg : PleyColors.Muted;
            label.TextColor = ink;
            if (icon != null) icon.Color = selected && !focused ? PleyColors.Accent : ink;
            if (count != null) count.TextColor = LiveViews.Alpha(ink, 0.7f);
        }

        protected override void Dispose(DisposeTypes type)
        {
            if (Disposed) return;
            // The column keeps the row's focus: its view comes and goes as the column scrolls.
            item.FocusChanged -= Look;
            base.Dispose(type);
        }
    }
}

/// <summary>
/// A programme's details in a side panel (the Android app's ProgramPanel): the channel; the
/// programme's picture, title, episode, day and times with its badges, how far it is and what the
/// guide says; then Watch and the favourite. Without a programme, the channel alone.
/// </summary>
sealed class ProgramPanelSheet : Sheet
{
    const float Width = 360, Left = 24;

    readonly Shell shell;
    readonly ButtonView favoriteButton;
    LiveChannel current;
    bool closed;

    public ProgramPanelSheet(Shell shell, Images images, LiveChannel channel, string name, LiveProgram? program, string origin, long now, Action watch, Action<LiveChannel> toggleFavorite)
        : base("program")
    {
        this.shell = shell;
        Gone += () => closed = true;
        current = channel;
        View.BackgroundColor = PleyColors.Scrim;
        var card = new View
        {
            Size = new Size(Ui.Dp(Width), Ui.Dp(PleyTv.CanvasHeight)),
            Position = new Position(Ui.Dp(PleyTv.CanvasWidth - Width), 0),
            BackgroundColor = PleyColors.Surface2,
            CornerRadius = new Vector4(Ui.Dp(PleyShape.DialogRadius * 2), 0, 0, Ui.Dp(PleyShape.DialogRadius * 2)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        View.Add(card);
        var inner = Width - Left - PleyTv.SafeHorizontal;
        float y = PleyTv.SafeVertical;

        var logo = new ChannelLogoView(channel, origin, images, 64, 38);
        logo.Position = new Position(Ui.Dp(Left), Ui.Dp(y));
        card.Add(logo);
        var heading = Text.Label(LiveText.NumberAndName(channel, name), PleyType.Label, PleyColors.Muted, width: inner - 76);
        heading.Position = new Position(Ui.Dp(Left + 76), Ui.Dp(y + (38 - PleyType.Label.Line) / 2));
        card.Add(heading);
        y += 38;

        // The buttons at the foot; the text above them takes the room left.
        var watchButton = new ButtonView(program == null || program.AiringAt(now) ? LiveText.WatchNow : LiveText.WatchChannel, ButtonKind.Pill, () =>
        {
            Close();
            watch();
        }, iconName: "play");
        favoriteButton = new ButtonView(FavoriteLabel(), ButtonKind.Pill, () =>
        {
            toggleFavorite(current);
            current = current with { Favorite = !current.Favorite };
            favoriteButton!.SetText(FavoriteLabel());
        }, iconName: "star");
        var side = watchButton.WidthDp + PleyTv.ButtonGap + Math.Max(favoriteButton.WidthDp, 210) <= inner;
        var buttonsTop = PleyTv.CanvasHeight - PleyTv.SafeVertical - PleyTv.ButtonHeight - (side ? 0 : PleyTv.ButtonHeight + PleyTv.ButtonGap);
        var buttons = new FocusGroup(side ? Axis.Row : Axis.Column, "buttons");
        watchButton.Position = new Position(Ui.Dp(Left), Ui.Dp(buttonsTop));
        favoriteButton.Position = side
            ? new Position(Ui.Dp(Left + watchButton.WidthDp + PleyTv.ButtonGap), Ui.Dp(buttonsTop))
            : new Position(Ui.Dp(Left), Ui.Dp(buttonsTop + PleyTv.ButtonHeight + PleyTv.ButtonGap));
        card.Add(watchButton);
        card.Add(favoriteButton);
        buttons.Add(watchButton.Item);
        buttons.Add(favoriteButton.Item);
        Root.Add(buttons);

        if (program != null) Program(card, images, program, origin, now, inner, y, buttonsTop - 12);
        Tree.Focus(watchButton.Item);
    }

    string FavoriteLabel() => current.Favorite ? LiveText.FavoriteRemove : LiveText.FavoriteAdd;

    void Program(View card, Images images, LiveProgram program, string origin, long now, float inner, float y, float bottom)
    {
        var clock = LiveClock.Local;
        if (!string.IsNullOrEmpty(program.Image))
        {
            y += 14;
            var picture = new ImageView
            {
                Size = new Size(Ui.Dp(inner), Ui.Dp(160)),
                Position = new Position(Ui.Dp(Left), Ui.Dp(y)),
                CornerRadius = Ui.Dp(PleyShape.TileRadius),
                BackgroundColor = PleyColors.Surface3,
                DesiredWidth = (int)Ui.Dp(inner),
                DesiredHeight = (int)Ui.Dp(160),
                FittingMode = FittingModeType.ScaleToFill,
            };
            card.Add(picture);
            images.Load(new[] { Artwork.Absolute(program.Image!, origin) }, inner, file =>
            {
                if (file != null && !closed) picture.ResourceUrl = file;
            });
            y += 160;
        }
        y += 14;
        var title = Text.Label(program.Title, PleyType.Title, PleyColors.Fg, maxLines: 2, width: inner);
        title.VerticalAlignment = VerticalAlignment.Top;
        var titleHeight = Math.Min(title.GetHeightForWidth(Ui.Dp(inner)), Text.LineHeight(PleyType.Title) * 2);
        title.SizeHeight = titleHeight;
        title.Position = new Position(Ui.Dp(Left), Ui.Dp(y));
        card.Add(title);
        y += titleHeight / Ui.Scale;
        var episode = string.Join(" · ", new[] { program.EpisodeLabel, program.Subtitle }.Where(s => !string.IsNullOrEmpty(s)));
        if (episode.Length > 0)
        {
            var line = Text.Label(episode, PleyType.Body, PleyColors.Muted, width: inner);
            line.Position = new Position(Ui.Dp(Left), Ui.Dp(y + 2));
            card.Add(line);
            y += 2 + PleyType.Body.Line;
        }
        y += 8;
        // The day and times, then the badges: on now, Live, New, the rating.
        float x = Left;
        var when = LiveViews.Fitted($"{clock.Day(program.Start, now)} · {clock.Span(program)}", PleyType.Caption, PleyColors.Muted, inner);
        when.Position = new Position(Ui.Dp(x), Ui.Dp(y));
        card.Add(when);
        x += when.SizeWidth / Ui.Scale + 6;
        var badges = new List<View>();
        if (program.AiringAt(now)) badges.Add(LiveViews.Badge(LiveText.OnNow, true));
        if (program.IsLive) badges.Add(LiveViews.Badge(LiveText.BadgeLive, true));
        if (program.IsNew) badges.Add(LiveViews.Badge(LiveText.BadgeNew, false));
        if (!string.IsNullOrEmpty(program.Rating))
        {
            var rating = Text.Label(program.Rating!, PleyType.Badge, PleyColors.Muted);
            var w = rating.NaturalSize.Width + Ui.Dp(10);
            var box = new View
            {
                Size = new Size(w, Ui.Dp(PleyType.Badge.Line + 4)),
                CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
                BorderlineWidth = Ui.Dp(1),
                BorderlineColor = PleyColors.Outline,
                BorderlineOffset = -1,
                BackgroundColor = Color.Transparent,
            };
            rating.Size = new Size(w, box.SizeHeight);
            rating.HorizontalAlignment = HorizontalAlignment.Center;
            box.Add(rating);
            badges.Add(box);
        }
        var rowHeight = PleyType.Caption.Line;
        foreach (var badge in badges)
        {
            var w = badge.SizeWidth / Ui.Scale;
            if (x + w > Left + inner)
            {
                x = Left;
                y += rowHeight + 6;
            }
            badge.Position = new Position(Ui.Dp(x), Ui.Dp(y + (rowHeight - badge.SizeHeight / Ui.Scale) / 2));
            card.Add(badge);
            x += w + 6;
        }
        y += rowHeight;
        if (program.AiringAt(now))
        {
            var left = LiveViews.Fitted(LiveText.TimeLeft(program, now), PleyType.Caption, PleyColors.Subtle, 120);
            var leftWidth = left.SizeWidth / Ui.Scale;
            var bar = LiveViews.Progress(program, now, inner - leftWidth - 10, PleyColors.Surface4);
            bar.Position = new Position(Ui.Dp(Left), Ui.Dp(y + 10 + (PleyType.Caption.Line - 3) / 2));
            card.Add(bar);
            left.Position = new Position(Ui.Dp(Left + inner - leftWidth), Ui.Dp(y + 10));
            card.Add(left);
            y += 10 + PleyType.Caption.Line;
        }
        y += 10;
        var ended = program.Stop <= now;
        var room = bottom - y - (ended ? PleyType.Caption.Line + 8 : 0);
        var lines = Math.Max(1, Math.Min(6, (int)(room / PleyType.Body.Line)));
        var described = !string.IsNullOrWhiteSpace(program.Description);
        var text = Text.Label(described ? program.Description! : LiveText.NoDescription, PleyType.Body, described ? PleyColors.Fg : PleyColors.Subtle, maxLines: lines, width: inner);
        text.VerticalAlignment = VerticalAlignment.Top;
        var textHeight = Math.Min(text.GetHeightForWidth(Ui.Dp(inner)), Text.LineHeight(PleyType.Body) * lines);
        text.SizeHeight = textHeight;
        text.Position = new Position(Ui.Dp(Left), Ui.Dp(y));
        card.Add(text);
        y += textHeight / Ui.Scale;
        if (ended)
        {
            var end = Text.Label(LiveText.Ended, PleyType.Caption, PleyColors.Subtle, width: inner);
            end.Position = new Position(Ui.Dp(Left), Ui.Dp(y + 8));
            card.Add(end);
        }
    }

    public void Show() => shell.Push(this);

    public void Close() => shell.Pop(this);
}
