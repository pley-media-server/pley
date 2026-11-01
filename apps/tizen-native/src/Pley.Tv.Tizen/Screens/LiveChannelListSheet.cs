using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The live player's channel list in a side panel (the Android app's ChannelList): a category's
/// channels a page at a time as they scroll, what's on each, the channel playing first in focus.
/// It opens on the category zapped through; its heading opens the categories to list another's,
/// and the search looks among every channel. A channel chosen plays, and channel up and down go
/// through the category it was chosen in.
/// </summary>
sealed class LiveChannelListSheet : Sheet
{
    const float Width = 360, Left = 24, HeadingHeight = 40, RowHeight = 52, RowGap = 4;

    readonly Shell shell;
    readonly LivePlayerModel model;
    readonly Images images;
    readonly Action<LiveChannel, LiveCategory> tune;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly float inner = Width - Left - PleyTv.SafeHorizontal;
    readonly float bodyTop = PleyTv.SafeVertical + HeadingHeight + 8;
    readonly View card;
    readonly View body = new View();
    readonly FocusGroup bodyGroup = new FocusGroup(Axis.Column, "body");
    readonly HeadingView heading;
    readonly TextInput search;
    readonly ListView list;
    CategoryColumnView? picker;
    bool picking;
    bool closed;

    public LiveChannelListSheet(Shell shell, LivePlayerModel model, Images images, Action<LiveChannel, LiveCategory> tune) : base("channels")
    {
        this.shell = shell;
        this.model = model;
        this.images = images;
        this.tune = tune;
        View.BackgroundColor = PleyColors.Scrim;
        card = new View
        {
            Size = new Size(Ui.Dp(Width), Ui.Dp(PleyTv.CanvasHeight)),
            Position = new Position(Ui.Dp(PleyTv.CanvasWidth - Width), 0),
            BackgroundColor = PleyColors.Surface2,
            CornerRadius = new Vector4(Ui.Dp(PleyShape.DialogRadius * 2), 0, 0, Ui.Dp(PleyShape.DialogRadius * 2)),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        View.Add(card);
        heading = new HeadingView(inner, () =>
        {
            picking = !picking;
            Build(focusBody: true);
        });
        heading.Position = new Position(Ui.Dp(Left), Ui.Dp(PleyTv.SafeVertical));
        card.Add(heading);
        body.Position = new Position(Ui.Dp(Left), Ui.Dp(bodyTop));
        card.Add(body);
        Root.Add(heading.Item);
        Root.Add(bodyGroup);

        search = new TextInput(shell.Input, inner, LiveText.SearchChannels, iconName: "search");
        search.Changed += q =>
        {
            if (!closed) model.SearchList(q);
        };
        var listTop = TextInput.Height + 8;
        list = new ListView(model, images, inner, PleyTv.CanvasHeight - bodyTop - listTop, Choose) { PositionY = Ui.Dp(listTop) };

        watches.Add(model.ListCategory.Watch(_ => Heading()));
        watches.Add(model.Groups.Watch(_ =>
        {
            if (picking) picker?.Show(model.Groups.Value, model.ListCategory.Value);
        }));
        watches.Add(model.Listed.Watch(_ => list.Relisted()));
        watches.Add(model.List.Watch(p => list.SetPages(p)));
        watches.Add(model.ListIndex.Watch(i => list.Place(i)));
        watches.Add(model.Now.Watch(n => list.SetNow(n)));
        watches.Add(model.Channel.Watch(_ => list.Draw()));
        Back = () =>
        {
            if (!picking) return false;
            picking = false;
            Build(focusBody: true);
            return true;
        };
        Gone += () =>
        {
            closed = true;
            foreach (var w in watches) w.Dispose();
            watches.Clear();
            // Put aside while picking a category: not under the sheet's view, so not let go with it.
            if (search.GetParent() == null) search.Dispose();
            if (list.GetParent() == null) list.Dispose();
        };
        Build(focusBody: true);
    }

    public void Show() => shell.Push(this);

    void Heading() => heading.Set(picking, model.ListCategory.Value);

    /// <summary>The categories while picking another, else the search and the channels; focus into them when asked.</summary>
    void Build(bool focusBody)
    {
        Heading();
        if (search.GetParent() != null) body.Remove(search);
        if (list.GetParent() != null) body.Remove(list);
        if (picker != null)
        {
            picker.Dispose();
            picker = null;
        }
        bodyGroup.Clear();
        if (picking)
        {
            picker = new CategoryColumnView(Tree, inner, PleyTv.CanvasHeight - bodyTop)
            {
                Chosen = category =>
                {
                    if (search.Value.Length > 0) search.Value = "";
                    model.ShowList(category);
                    picking = false;
                    Build(focusBody: true);
                },
            };
            body.Add(picker);
            bodyGroup.Add(picker.Group);
            picker.Show(model.Groups.Value, model.ListCategory.Value);
            if (focusBody && !picker.FocusShown()) Tree.Focus(heading.Item);
            return;
        }
        body.Add(search);
        body.Add(list);
        bodyGroup.Add(search.Item);
        bodyGroup.Add(list.Item);
        if (focusBody) Tree.Focus(list.Item);
    }

    void Choose(LiveChannel channel)
    {
        var from = model.Listed.Value;
        shell.Pop(this);
        tune(channel, from);
    }

    /// <summary>The list's heading: the category it shows and a chevron; while picking, "Channel groups" and a way back.</summary>
    sealed class HeadingView : View
    {
        readonly float width;
        ImageView? lead;
        readonly TextLabel label;
        ImageView? chevron;
        bool picking;
        LiveCategory? category;

        public HeadingView(float width, Action toggle)
        {
            this.width = width;
            Size = new Size(Ui.Dp(width), Ui.Dp(HeadingHeight));
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            Item = new FocusItem("heading") { Selected = toggle };
            label = Text.Label("", PleyType.Title, PleyColors.Fg);
            Add(label);
            Item.FocusChanged += _ => Draw();
        }

        public FocusItem Item { get; }

        public void Set(bool picking, LiveCategory category)
        {
            this.picking = picking;
            this.category = category;
            Draw();
        }

        void Draw()
        {
            if (Disposed) return;
            var focused = Item.Focused;
            var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
            BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
            lead?.Dispose();
            chevron?.Dispose();
            lead = chevron = null;
            float x = 8;
            if (picking) lead = Icons.Icon("arrow-left", 20, ink);
            else if (category is LiveCategory.Favorites) lead = Icons.Icon("star", 18, ink);
            if (lead != null)
            {
                var size = lead.SizeWidth / Ui.Scale;
                lead.Position = new Position(Ui.Dp(x), Ui.Dp((HeadingHeight - size) / 2));
                Add(lead);
                x += size + (picking ? 10 : 8);
            }
            var room = width - x - 8 - (picking ? 0 : 26);
            label.Text = Text.Fit(picking ? LiveText.ChannelGroups : LiveText.CategoryLabel(category ?? new LiveCategory.All()), PleyType.Title, room);
            label.TextColor = ink;
            label.Size = new Size(Ui.Dp(Math.Min(Text.Width(label), room)), Ui.Dp(HeadingHeight));
            label.PositionX = Ui.Dp(x);
            if (picking) return;
            chevron = Icons.Icon("chevron-down", 18, ink);
            chevron.Position = new Position(label.PositionX + label.SizeWidth + Ui.Dp(8), Ui.Dp((HeadingHeight - 18) / 2));
            Add(chevron);
        }
    }

    /// <summary>
    /// The channels, one focus item moved through rows of 52 dp, 4 apart: the channel playing
    /// marked with Surface3, the focused one filled with the accent; only the rows near the screen
    /// have views. Up from the first goes to the search.
    /// </summary>
    sealed class ListView : View
    {
        readonly LivePlayerModel model;
        readonly Images images;
        readonly float width;
        readonly float height;
        readonly Action<LiveChannel> choose;
        readonly View rows = new View();
        readonly Dictionary<int, ListRow> drawn = new Dictionary<int, ListRow>();
        readonly TextLabel none;
        ChannelPages pages = ChannelPages.Empty;
        long now;
        int index;
        int top;
        bool moved;

        public ListView(LivePlayerModel model, Images images, float width, float height, Action<LiveChannel> choose)
        {
            this.model = model;
            this.images = images;
            this.width = width;
            this.height = height;
            this.choose = choose;
            now = model.Now.Value;
            Size = new Size(Ui.Dp(width), Ui.Dp(height));
            ClippingMode = ClippingModeType.ClipChildren;
            Add(rows);
            none = Text.Label(LiveText.NoMatch, PleyType.Label, PleyColors.Subtle, width: width - 16);
            none.Position = new Position(Ui.Dp(8), Ui.Dp(8));
            none.Hide();
            Add(none);
            Item = new FocusItem("list")
            {
                Selected = () =>
                {
                    if (pages[index] is { } c) choose(c);
                },
                OwnKey = (key, down) =>
                {
                    if (key == "Up")
                    {
                        if (!down) return true;
                        if (index == 0) return false;
                        Move(index - 1);
                        return true;
                    }
                    if (key == "Down")
                    {
                        if (down && index + 1 < Total) Move(index + 1);
                        return true;
                    }
                    if (key is "Left" or "Right") return true;
                    return false;
                },
            };
            Item.FocusChanged += _ => Draw();
        }

        public FocusItem Item { get; }

        int Total => pages.Total ?? 0;

        int Shown => (int)Math.Ceiling(height / (RowHeight + RowGap));

        /// <summary>Another list (a search, another category): from its top until the channel playing is placed.</summary>
        public void Relisted()
        {
            index = 0;
            moved = false;
            ScrollTo(0, animate: false);
            Clear();
        }

        public void SetPages(ChannelPages value)
        {
            pages = value;
            if (Total > 0 && index >= Total) index = Total - 1;
            Draw();
        }

        /// <summary>The channel playing is at <paramref name="at"/> in the list: focus starts there, two rows from the top.</summary>
        public void Place(int? at)
        {
            if (at == null || moved) return;
            index = at.Value;
            ScrollTo(Math.Max(0, index - 2), animate: false);
            Draw();
        }

        public void SetNow(long value)
        {
            now = value;
            foreach (var row in drawn.Values) row.Now(now);
        }

        void Move(int to)
        {
            moved = true;
            index = to;
            if (index < top) ScrollTo(index, animate: true);
            else if (index >= top + Shown - 1) ScrollTo(index - Shown + 2, animate: true);
            Draw();
        }

        void ScrollTo(int row, bool animate)
        {
            top = Math.Max(0, row);
            var y = -Ui.Dp(top * (RowHeight + RowGap));
            if (animate)
            {
                var a = new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
                a.AnimateTo(rows, "PositionY", y);
                a.Play();
            }
            else rows.PositionY = y;
            model.ShowingListRows(top, top + Shown);
        }

        void Clear()
        {
            foreach (var row in drawn.Values) row.Dispose();
            drawn.Clear();
        }

        public void Draw()
        {
            if (Disposed) return;
            if (pages.Total == 0) none.Show();
            else none.Hide();
            var first = Math.Max(0, top - 1);
            var last = Math.Min(Total - 1, top + Shown + 1);
            foreach (var i in drawn.Keys.Where(i => i < first || i > last).ToList())
            {
                drawn[i].Dispose();
                drawn.Remove(i);
            }
            var playing = model.State.Value.ChannelId;
            for (var i = first; i <= last; i++)
            {
                var c = pages[i];
                if (!drawn.TryGetValue(i, out var row) || !Equals(row.Channel, c))
                {
                    row?.Dispose();
                    row = new ListRow(c, c != null ? LiveRules.ChannelName(c, model.Listed.Value) : "", model.Origin, images, width, now);
                    row.PositionY = Ui.Dp(i * (RowHeight + RowGap));
                    rows.Add(row);
                    drawn[i] = row;
                }
                row.Look(Item.Focused && i == index, c != null && c.Id == playing);
            }
        }
    }

    /// <summary>One channel: its logo, number and name, what's on, and a star for a favourite.</summary>
    sealed class ListRow : View
    {
        readonly TextLabel? name;
        readonly TextLabel? onNow;
        readonly ImageView? star;

        public ListRow(LiveChannel? channel, string label, string origin, Images images, float width, long now)
        {
            Channel = channel;
            Size = new Size(Ui.Dp(width), Ui.Dp(RowHeight));
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            if (channel == null)
            {
                BackgroundColor = LiveViews.Alpha(Color.White, 0.04f);
                return;
            }
            var logo = new ChannelLogoView(channel, origin, images, 52, 32, well: LiveViews.Alpha(Color.White, 0.08f));
            logo.Position = new Position(Ui.Dp(10), Ui.Dp((RowHeight - 32) / 2));
            Add(logo);
            var x = 10 + 52 + 12;
            var right = width - 10 - (channel.Favorite ? 14 + 8 : 0);
            name = Text.Label(LiveText.NumberAndName(channel, label), PleyType.Label, PleyColors.Fg, width: right - x);
            onNow = Text.Label("", PleyType.Caption, LiveViews.Alpha(PleyColors.Fg, 0.75f), width: right - x);
            var block = PleyType.Label.Line + PleyType.Caption.Line;
            name.Position = new Position(Ui.Dp(x), Ui.Dp((RowHeight - block) / 2));
            onNow.Position = new Position(Ui.Dp(x), Ui.Dp((RowHeight - block) / 2 + PleyType.Label.Line));
            Add(name);
            Add(onNow);
            if (channel.Favorite)
            {
                star = Icons.Icon("star", 14, PleyColors.Fg);
                star.Position = new Position(Ui.Dp(width - 10 - 14), Ui.Dp((RowHeight - 14) / 2));
                Add(star);
            }
            Now(now);
        }

        public LiveChannel? Channel { get; }

        public void Now(long now)
        {
            if (Disposed || Channel == null || onNow == null || name == null) return;
            var on = Channel.OnAt(now);
            Text.Show(onNow, on?.Title ?? "");
            // Without what's on, the name sits in the middle.
            var block = PleyType.Label.Line + (on != null ? PleyType.Caption.Line : 0);
            name.PositionY = Ui.Dp((RowHeight - block) / 2);
        }

        public void Look(bool focused, bool current)
        {
            if (Disposed || Channel == null) return;
            BackgroundColor = focused ? PleyColors.Accent : current ? PleyColors.Surface3 : Color.Transparent;
            var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
            if (name != null) name.TextColor = ink;
            if (onNow != null) onNow.TextColor = LiveViews.Alpha(ink, 0.75f);
            if (star != null) star.Color = ink;
        }
    }
}
