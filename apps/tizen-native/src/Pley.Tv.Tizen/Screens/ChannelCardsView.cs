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

/// <summary>
/// Live TV's Channels view (the Android app's ChannelTiles): the channels as tiles, three across
/// beside the categories, a page loaded at a time, opening on the channel last watched. Focus is
/// one item moved through the cards, so only the rows near the screen have views; the focused
/// row stays at the top with the next peeking in, as in a library's grid.
/// </summary>
sealed class ChannelCardsView : View
{
    const int Columns = 3;
    /// <summary>Room above the first row for the focused tile's growth and ring.</summary>
    const float Room = 14, RowGap = 20;

    static readonly TileKind Kind = ChannelCard.Kind;

    readonly LiveTvModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly LiveCategory listed;
    readonly string origin;
    readonly Func<bool> toCategories;
    readonly Action<LiveChannel> watch;
    readonly Action<LiveChannel, LiveProgram?> details;
    readonly View content = new View();
    readonly Dictionary<int, ChannelCard> cards = new Dictionary<int, ChannelCard>();
    readonly float pitch = Kind.Height + PleyTv.TilesToCaption + PleyTv.CaptionHeight + RowGap;
    readonly float height;
    ChannelPages pages = ChannelPages.Empty;
    long now;
    int index;
    int topRow;
    Landing? placed;

    public ChannelCardsView(LiveTvModel model, Images images, KeyRepeatGate gate, LiveCategory listed, string origin, float widthDp, float heightDp,
        Func<bool> toCategories, Action<LiveChannel> watch, Action<LiveChannel, LiveProgram?> details)
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.listed = listed;
        this.origin = origin;
        this.toCategories = toCategories;
        this.watch = watch;
        this.details = details;
        height = heightDp;
        now = model.Now.Value;
        Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp));
        ClippingMode = ClippingModeType.ClipChildren;
        content.PositionY = Ui.Dp(Room);
        Add(content);
        Item = new FocusItem("channels")
        {
            Selected = () =>
            {
                if (pages[index] is { } c) watch(c);
            },
            OwnKey = OnKey,
        };
        Item.LongSelected = Details;
        Item.Menu = Details;
        Item.FocusChanged += _ => Draw();
    }

    public FocusItem Item { get; }

    int Total => pages.Total ?? 0;

    public void SetPages(ChannelPages value)
    {
        Profile.Mark("live cards: pages");
        pages = value;
        if (index >= Total) index = Math.Max(0, Total - 1);
        Draw();
    }

    public void SetNow(long value)
    {
        now = value;
        foreach (var card in cards.Values) card.Now(now);
    }

    /// <summary>The cards open at the channel last watched (once per landing).</summary>
    public void Land(Landing? landing)
    {
        if (landing == null || landing.Category != model.State.Value.Category || placed == landing) return;
        placed = landing;
        index = Math.Max(0, Math.Min(landing.Index ?? 0, Math.Max(0, Total - 1)));
        Scroll(animate: false);
        Draw();
    }

    bool OnKey(string key, bool down)
    {
        if (Disposed) return false;
        var column = index % Columns;
        var row = index / Columns;
        switch (key)
        {
            case "Left":
                if (!down) return true;
                if (column == 0) Scrolling.Left(gate, toCategories);
                else Move(index - 1);
                return true;
            case "Right":
                if (down && column < Columns - 1 && index + 1 < Total) Move(index + 1);
                return true;
            case "Up":
                if (!down) return true;
                if (row == 0) return false;
                Move(index - Columns);
                return true;
            case "Down":
                if (!down) return true;
                if (index + Columns < Total) Move(index + Columns);
                // A shorter last row: Down from above its end lands on its last card.
                else if (row < (Total - 1) / Columns) Move(Total - 1);
                return true;
        }
        if (RemoteKeys.Info(key))
        {
            if (!down) Details();
            return true;
        }
        return false;
    }

    void Details()
    {
        if (pages[index] is { } c) details(c, c.OnAt(now));
    }

    void Move(int to)
    {
        index = to;
        Scroll(animate: true);
        Draw();
    }

    void Scroll(bool animate)
    {
        var row = index / Columns;
        if (row != topRow || !animate)
        {
            topRow = row;
            var y = Ui.Dp(Room - row * pitch);
            if (animate)
            {
                var a = Scrolling.Glide(gate);
                a.AnimateTo(content, "PositionY", y);
                a.PlayHeld();
            }
            else content.PositionY = y;
        }
        var rows = (int)Math.Ceiling(height / pitch) + 1;
        model.ShowingRows(topRow * Columns, (topRow + rows) * Columns - 1);
    }

    void Draw()
    {
        if (Disposed) return;
        var rows = (int)Math.Ceiling(height / pitch) + 1;
        var first = Math.Max(0, topRow - 1) * Columns;
        var last = Math.Min(Total - 1, (topRow + rows) * Columns - 1);
        foreach (var i in cards.Keys.Where(i => i < first || i > last).ToList())
        {
            cards[i].Dispose();
            cards.Remove(i);
        }
        for (var i = first; i <= last; i++)
        {
            var c = pages[i];
            if (!cards.TryGetValue(i, out var card) || !Equals(card.Channel, c))
            {
                card?.Dispose();
                card = new ChannelCard(c, c != null ? LiveRules.ChannelName(c, listed) : "", origin, images, now);
                card.Position = new Position(Ui.Dp(i % Columns * (Kind.Width + PleyTv.WideTileGap)), Ui.Dp(i / Columns * pitch));
                content.Add(card);
                cards[i] = card;
            }
            card.Focus(Item.Focused && i == index);
        }
    }
}

/// <summary>
/// A channel's tile: its logo (or name) with its number, a star when a favourite and how far
/// what's on is; captioned with its name and what's on. Focused, it grows to 1.08 in the
/// accent ring and its name turns bold. A channel still loading is a still skeleton. Live TV's
/// Channels view draws them, and search's Live TV row.
/// </summary>
sealed class ChannelCard : View
{
    public static readonly TileKind Kind = TileKind.Episode;

    readonly View frame;
    readonly View? ring;
    readonly View well;
    readonly TextLabel? name;
    readonly TextLabel? onNow;
    View? progress;
    LiveProgram? progressOf;
    bool focused;
    Animation? look;

    /// <param name="logo">Gives up the logo while it comes: the card's row went.</param>
    public ChannelCard(LiveChannel? channel, string label, string origin, Images images, long now, CancellationToken logo = default)
    {
        Channel = channel;
        var w = Ui.Dp(Kind.Width);
        var h = Ui.Dp(Kind.Height);
        Size = new Size(w, h + Ui.Dp(PleyTv.TilesToCaption + PleyTv.CaptionHeight));
        frame = new View { Size = new Size(w, h), PivotPoint = global::Tizen.NUI.PivotPoint.Center, PositionUsesPivotPoint = false };
        Add(frame);
        if (channel == null)
        {
            well = new View { Size = new Size(w, h), CornerRadius = Ui.Dp(PleyShape.TileRadius), BackgroundColor = PleyColors.Surface2 };
            frame.Add(well);
            var captionTop = h + Ui.Dp(PleyTv.TilesToCaption);
            Add(new View { Position = new Position(0, captionTop + Ui.Dp(3)), Size = new Size(w * 0.6f, Ui.Dp(12)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
            Add(new View { Position = new Position(0, captionTop + Ui.Dp(23)), Size = new Size(w * 0.35f, Ui.Dp(12)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = PleyColors.Surface2 });
            return;
        }
        var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
        ring = new View
        {
            Size = new Size(w + 2 * reach, h + 2 * reach),
            Position = new Position(-reach, -reach),
            BackgroundColor = Color.Transparent,
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1f,
            CornerRadius = Ui.Dp(PleyShape.TileRadius) + reach,
            Opacity = 0,
        };
        frame.Add(ring);
        well = new ChannelLogoView(channel, origin, images, Kind.Width, Kind.Height, name: true, style: PleyType.Title, cancel: logo);
        frame.Add(well);
        if (channel.Number is { } number)
        {
            var text = Text.Label(number, PleyType.Badge, PleyColors.Fg);
            var box = new View
            {
                Position = new Position(Ui.Dp(6), Ui.Dp(6)),
                Size = new Size(text.NaturalSize.Width + Ui.Dp(10), Ui.Dp(PleyType.Badge.Line + 4)),
                BackgroundColor = PleyColors.OnArt,
                CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
            };
            text.Size = new Size(box.SizeWidth, box.SizeHeight);
            text.HorizontalAlignment = HorizontalAlignment.Center;
            box.Add(text);
            well.Add(box);
        }
        if (channel.Favorite)
        {
            var star = Icons.Icon("star", 16, PleyColors.Accent);
            star.Position = new Position(w - Ui.Dp(6 + 16), Ui.Dp(6));
            well.Add(star);
        }
        var top = h + Ui.Dp(PleyTv.TilesToCaption);
        name = Text.Label(label, PleyType.Caption, PleyColors.Fg, width: Kind.Width);
        name.PositionY = top;
        Add(name);
        onNow = Text.Label("", PleyType.Caption, PleyColors.Muted, width: Kind.Width);
        onNow.PositionY = top + Ui.Dp(PleyType.Caption.Line);
        Add(onNow);
        Now(now);
    }

    public LiveChannel? Channel { get; }

    /// <summary>What's on and how far it is, at <paramref name="now"/>.</summary>
    public void Now(long now)
    {
        if (Disposed || Channel == null || onNow == null) return;
        Text.Show(onNow, LiveText.OnNowOf(Channel, now));
        var on = Channel.OnAt(now);
        // The same programme on: its bar moves in place, rather than made again at each 15 s tick.
        if (on != null && progress != null && progressOf is { } was && was.Start == on.Start && was.Stop == on.Stop)
        {
            if (progress.ChildCount > 0 && progress.GetChildAt(0) is View fill) fill.SizeWidth = Ui.Dp(Kind.Width * on.ProgressAt(now));
            return;
        }
        progress?.Dispose();
        progress = null;
        progressOf = on;
        if (on == null) return;
        progress = LiveViews.Progress(on, now, Kind.Width, PleyColors.ProgressTrackOnArt);
        progress.PositionY = Ui.Dp(Kind.Height - 3);
        well.Add(progress);
    }

    public void Focus(bool on)
    {
        if (Disposed || focused == on) return;
        focused = on;
        if (on) RaiseToTop();
        if (name != null) name.FontStyle = Text.Weight(on ? PleyType.Bold : PleyType.Caption.Weight);
        var scale = on ? PleyShape.TileScale : 1f;
        look?.Stop();
        look = new Animation(on ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
        {
            DefaultAlphaFunction = new AlphaFunction(on ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
        };
        look.AnimateTo(frame, "Scale", new Vector3(scale, scale, 1));
        if (ring != null) look.AnimateTo(ring, "Opacity", on ? 1f : 0f);
        look.Play();
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        // Only when disposed by the app: collected, the animation may have gone first (Kit/Held.cs).
        if (type == DisposeTypes.Explicit) look?.Stop();
        base.Dispose(type);
    }
}
