using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Now Playing on TV (design spec §4.10; the Android app's TvNowPlayingScreen): the album art at 300
/// dp at (96, 58), and 32 dp beside it the title, the artist and album, a status line, the scrubber
/// with the times under it, then Shuffle, Previous, Play/Pause (64 dp, the first focus), Next and
/// the sleep timer, then "Up next" and the queue from the track playing on. Over a faint copy of
/// the art at 20 %. Focused, the scrubber's Left and Right seek 10 s and OK plays or pauses; OK on
/// a track of the queue plays it. The music plays on when the page is left. After two minutes
/// without a key the page dims to 30 %, and the next key wakes it. Nothing playing: "Nothing is
/// playing", with Back.
/// </summary>
sealed class NowPlayingScreen : ShellPage
{
    const float ArtTop = 58, Art = 300, ArtGap = 32, PlaySize = 64, ControlGap = 16, QueueBottom = PleyTv.SafeVertical + 60;
    const float ColumnLeft = PleyTv.Gutter + Art + ArtGap, ColumnWidth = PleyTv.CanvasWidth - PleyTv.SafeHorizontal - ColumnLeft;

    readonly NowPlayingModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly IDisposable keys;
    readonly Timer tick = new Timer((uint)NowPlayingModel.PositionEveryMs);
    readonly FaintArtView faint;
    readonly View body = new View();
    readonly View dim = new View { BackgroundColor = Color.Black, Opacity = 0 };
    string shownKind = "";
    string? shownArt;
    string? shownTitle;
    ArtWell? art;
    TextLabel? title;
    TextLabel? byline;
    TextLabel? status;
    Scrubber? scrubber;
    TextLabel? played;
    TextLabel? left;
    View? controls;
    RoundButton? shuffle;
    RoundButton? next;
    RoundButton? sleep;
    PlayPauseButton? playPause;
    TextLabel? queueHeader;
    TrackListView? queue;
    FocusGroup? controlsGroup;
    int shownCurrent = -1;
    long lastKey = Environment.TickCount64;
    bool dimmed;
    Animation? dimming;
    bool gone;

    public NowPlayingScreen(TvShell shell, Shell layers, NowPlayingModel model, Images images, KeyRepeatGate gate)
        : base(shell, new PageKey.NowPlaying(), "now-playing")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        View.BackgroundColor = PleyColors.Bg;
        faint = new FaintArtView(images, 0.2f);
        View.Add(faint);
        View.Add(body);
        dim.Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
        View.Add(dim);
        // Every key wakes the page; none is taken here.
        keys = layers.Input.Push((key, down) =>
        {
            Woken();
            return false;
        });
        watches.Add(model.State.Watch(Show));
        watches.Add(model.Sleep.Watch(_ => Sleep()));
        tick.Tick += (s, e) =>
        {
            if (gone) return false;
            Position();
            if (!dimmed && Environment.TickCount64 - lastKey >= NowPlayingModel.DimAfterMs) Dim(true);
            return true;
        };
        tick.Start();
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    public override void Gone()
    {
        gone = true;
        tick.Stop();
        dimming?.Stop();
        keys.Dispose();
        foreach (var w in watches) w.Dispose();
        model.Dispose();
    }

    void Woken()
    {
        lastKey = Environment.TickCount64;
        if (dimmed) Dim(false);
    }

    /// <summary>To 30 % (black at 70 % over the page) after two minutes without a key, and back at the next.</summary>
    void Dim(bool on)
    {
        dimmed = on;
        dimming?.Stop();
        dimming = new Animation(on ? PleyMotion.RestMs : PleyMotion.OverlayMs);
        dimming.AnimateTo(dim, "Opacity", on ? 1f - NowPlayingModel.Dimmed : 0f);
        dimming.Play();
    }

    void Show(NowPlayingState state)
    {
        if (gone) return;
        faint.Show(state.Now?.ArtUrl);
        if (state.Now is not { } now)
        {
            if (shownKind == "nothing") return;
            shownKind = "nothing";
            Clear();
            var row = MusicKit.Message(body, MusicText.NothingPlaying, MusicText.NothingPlayingBody, MusicText.Back, () =>
            {
                if (Shell.Top == this) Shell.Back?.Invoke();
            });
            row.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
            Group.Add(row.Group);
            if (Shell.Top == this) Tree.Enter(row.Group);
            return;
        }
        var first = shownKind != "playing";
        if (first)
        {
            Clear();
            Build();
        }
        shownKind = "playing";
        Track(now);
        Controls(now);
        Queue(state, now);
        Position();
        if (first && Shell.Top == this) Tree.Focus(playPause!.Item);
    }

    void Clear()
    {
        Group.Clear();
        MusicKit.Empty(body);
        art = null;
        title = byline = status = played = left = queueHeader = null;
        scrubber = null;
        controls = null;
        shuffle = next = sleep = null;
        playPause = null;
        queue = null;
        controlsGroup = null;
        shownArt = null;
        shownTitle = null;
        shownCurrent = -1;
    }

    /// <summary>The page's parts, made once while something plays; <see cref="Place"/> sets where they go.</summary>
    void Build()
    {
        title = PanelColumn.Lines("", PleyType.PageTitle, PleyColors.Fg, ColumnWidth, 2);
        byline = Text.Label("", PleyType.BodyLarge, PleyColors.Muted, width: ColumnWidth);
        status = Text.Label("", PleyType.Caption, PleyColors.Subtle, width: ColumnWidth);
        scrubber = new Scrubber(ColumnWidth);
        played = Text.Label("", PleyType.Caption, PleyColors.Fg, width: ColumnWidth / 2);
        left = Text.Label("", PleyType.Caption, PleyColors.Fg, width: ColumnWidth / 2);
        left.HorizontalAlignment = HorizontalAlignment.End;
        foreach (var view in new View[] { title, byline, status, scrubber, played, left }) body.Add(view);

        controls = new View();
        controlsGroup = new FocusGroup(Axis.Row, "controls");
        shuffle = new RoundButton(MusicText.Shuffle, "shuffle", () =>
        {
            if (model.State.Value.Now is { } n) model.SetShuffle(!n.Shuffle);
        });
        var previous = new RoundButton(MusicText.Previous, "skip-back", model.Previous);
        playPause = new PlayPauseButton(model.Toggle);
        next = new RoundButton(MusicText.Next, "skip-forward", model.Next);
        sleep = new RoundButton(model.SleepLabel, "moon", model.StepSleep);
        var x = 0f;
        foreach (var (view, item) in new (View, FocusItem)[] { (shuffle, shuffle.Item), (previous, previous.Item), (playPause, playPause.Item), (next, next.Item), (sleep, sleep.Item) })
        {
            // The round buttons sit 8 dp down beside Play/Pause, as on Android.
            view.Position = new Position(Ui.Dp(x), view == playPause ? 0 : Ui.Dp(8));
            controls.Add(view);
            controlsGroup.Add(item);
            x += view.SizeWidth / Ui.Scale + ControlGap;
        }
        controlsGroup.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
        controlsGroup.RememberChild(2);
        body.Add(controls);

        queueHeader = Text.Label(MusicText.Queue, PleyType.Label, PleyColors.Subtle, width: ColumnWidth - 12);
        body.Add(queueHeader);

        Group.Add(scrubber.Item);
        Group.Add(controlsGroup);
        Group.RememberChild(1);
        scrubber.Item.OwnKey = ScrubberKey;
    }

    /// <summary>
    /// Down the column: the title (PageTitle, 2 lines at most), 4 dp, the byline (BodyLarge Muted),
    /// the 22 dp status line, 14 dp, the scrubber (24) and the times under it, 16 dp, the controls
    /// (64 dp and the room for their labels), then "Up next" and the queue to the foot.
    /// </summary>
    void Place()
    {
        var y = ArtTop;
        title!.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(y));
        y += title.SizeHeight / Ui.Scale + 4;
        byline!.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(y));
        y += PleyType.BodyLarge.Line;
        status!.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(y + 4));
        y += 22 + 14;
        scrubber!.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(y));
        y += Scrubber.Height + 4;
        played!.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(y));
        left!.Position = new Position(Ui.Dp(ColumnLeft + ColumnWidth / 2), Ui.Dp(y));
        y += PleyType.Caption.Line + 16;
        controls!.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(y));
        y += PlaySize + MusicKit.ButtonLabelRoom;
        queueHeader!.Position = new Position(Ui.Dp(ColumnLeft + 12), Ui.Dp(y));
        y += PleyType.Label.Line + 4;
        QueueTop = y;
        if (queue == null) return;
        queue.PositionY = Ui.Dp(y);
        queue.Resize(PleyTv.CanvasHeight - y);
    }

    float QueueTop { get; set; }

    /// <summary>The art, the title, the byline and the status line.</summary>
    void Track(NowPlaying now)
    {
        if (now.ArtUrl != shownArt || art == null)
        {
            shownArt = now.ArtUrl;
            var well = new ArtWell(Art, PleyShape.DialogRadius, images, now.ArtUrl);
            well.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ArtTop));
            body.Add(well);
            if (art != null)
            {
                art.Unparent();
                art.Dispose();
            }
            art = well;
        }
        if (now.Title != shownTitle)
        {
            shownTitle = now.Title;
            title!.Text = now.Title;
            title.SizeHeight = Math.Min(Math.Max(title.GetHeightForWidth(Ui.Dp(ColumnWidth)), Text.LineHeight(PleyType.PageTitle)), Text.LineHeight(PleyType.PageTitle) * 2);
            Place();
        }
        Text.Show(byline!, now.Byline);
        Status(now);
    }

    void Status(NowPlaying now)
    {
        if (status == null) return;
        Text.Show(status, model.Status);
        status.TextColor = now.Error != null ? PleyColors.DangerText : PleyColors.Subtle;
    }

    /// <summary>Shuffle's label, Play/Pause's glyph, and Next only while there is a next track.</summary>
    void Controls(NowPlaying now)
    {
        shuffle!.SetText(model.ShuffleLabel);
        playPause!.Set(model.ShowsPause);
        var hadNext = next!.Item.Focused;
        next.Enabled = now.HasNext;
        if (hadNext && !now.HasNext) Tree.Focus(playPause.Item);
    }

    /// <summary>The sleep timer's label and its notice in the status line.</summary>
    void Sleep()
    {
        if (gone) return;
        sleep?.SetText(model.SleepLabel);
        if (model.State.Value.Now is { } now) Status(now);
    }

    /// <summary>
    /// The queue, shown with more than one track: the track playing comes to the top as it changes,
    /// unless someone is looking down the queue (focus is in it).
    /// </summary>
    void Queue(NowPlayingState state, NowPlaying now)
    {
        if (state.Queue.Count <= 1)
        {
            queueHeader!.Hide();
            if (queue != null)
            {
                var had = queue.Focused;
                Group.Remove(queue.Group);
                queue.Unparent();
                queue.Dispose();
                queue = null;
                if (had) Tree.Focus(playPause!.Item);
            }
            return;
        }
        queueHeader!.Show();
        if (queue == null)
        {
            queue = new TrackListView(ColumnWidth, PleyTv.CanvasHeight - QueueTop, gate, Pick, Tree.Focus, QueueBottom, "queue");
            queue.Position = new Position(Ui.Dp(ColumnLeft), Ui.Dp(QueueTop));
            queue.Group.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
            body.Add(queue);
            Group.Add(queue.Group);
            shownCurrent = -1;
        }
        queue.Set(state.Queue.Select(r => new TrackLine(LineKey(r.Position), r.Title, Number: r.Number, Subtitle: r.Subtitle, Duration: r.Duration,
            Current: r.Current, Playing: r.Current && now.Playing)).ToList());
        var current = state.Queue.FirstOrDefault(r => r.Current)?.Position ?? 0;
        if (current != shownCurrent)
        {
            shownCurrent = current;
            if (queue.Focused) return;
            queue.ShowLine(LineKey(current));
            // Down from the controls lands on the track playing, at the top of the queue.
            queue.Remember(LineKey(current));
        }
    }

    static string LineKey(int position) => "queue-" + position.ToString(CultureInfo.InvariantCulture);

    void Pick(string key)
    {
        if (int.TryParse(key.Substring("queue-".Length), NumberStyles.Integer, CultureInfo.InvariantCulture, out var position)) model.JumpTo(position);
    }

    /// <summary>The scrubber and the times, read from the player: every half second while the page shows.</summary>
    void Position()
    {
        if (scrubber == null || played == null || left == null) return;
        var p = model.Position();
        var duration = p.DurationMs > 0 ? p.DurationMs : model.State.Value.Now?.DurationMs ?? 0;
        scrubber.Set(duration > 0 ? Math.Min(1f, (float)p.PositionMs / duration) : 0f, duration > 0 ? Math.Min(1f, (float)p.BufferedMs / duration) : 0f);
        var (playedText, leftText) = model.Times(p);
        played.Text = playedText;
        left.Text = leftText;
    }

    /// <summary>Keys on the scrubber: Left and Right seek 10 s, OK plays or pauses (as it comes up, as OK selects elsewhere).</summary>
    bool ScrubberKey(string key, bool down)
    {
        switch (key)
        {
            case "Left":
            case "Right":
                Woken();
                if (down)
                {
                    model.SeekBy(key == "Left" ? -1 : 1);
                    Position();
                }
                return true;
            case "Return":
            case "KP_Enter":
                Woken();
                if (!down) model.Toggle();
                return true;
            default:
                return false;
        }
    }

    /// <summary>
    /// The scrubber (design spec §2.7, as wide as the column): a 4 dp track at 25 %, the buffered
    /// part at 45 %, the played part in the accent, and a 10 dp thumb that grows to 12 with a 2 dp
    /// AccentBright ring 2 dp clear of it while focused.
    /// </summary>
    sealed class Scrubber : View
    {
        public const float Height = 24;
        const float Track = 4, Thumb = 10, ThumbFocused = 12;

        readonly float width;
        readonly View buffered;
        readonly View playedPart;
        readonly View thumb;
        readonly View ring;
        float fraction;

        public Scrubber(float widthDp)
        {
            width = widthDp;
            Size = new Size(Ui.Dp(widthDp), Ui.Dp(Height));
            Item = new FocusItem(MusicText.Position);
            var y = Ui.Dp((Height - Track) / 2);
            Add(Bar(widthDp, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.25f), y));
            buffered = Bar(0, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.45f), y);
            Add(buffered);
            playedPart = Bar(0, PleyColors.Accent, y);
            Add(playedPart);
            thumb = new View { BackgroundColor = PleyColors.Fg };
            Add(thumb);
            ring = new View { BorderlineWidth = Ui.Dp(PleyShape.ThumbRingStroke), BorderlineColor = PleyColors.AccentBright, BorderlineOffset = -1 };
            Add(ring);
            Item.FocusChanged += _ =>
            {
                if (!Disposed) Draw();
            };
            Draw();
        }

        public FocusItem Item { get; }

        static View Bar(float w, Color color, float y) => new View
        {
            Size = new Size(Ui.Dp(w), Ui.Dp(Track)),
            Position = new Position(0, y),
            CornerRadius = Ui.Dp(Track / 2),
            BackgroundColor = color,
        };

        public void Set(float played, float loaded)
        {
            fraction = played;
            buffered.SizeWidth = Ui.Dp(width * loaded);
            playedPart.SizeWidth = Ui.Dp(width * played);
            Draw();
        }

        void Draw()
        {
            var focused = Item.Focused;
            var d = focused ? ThumbFocused : Thumb;
            var r = d / 2;
            var x = Math.Max(r, Math.Min(width - r, width * fraction));
            thumb.Size = new Size(Ui.Dp(d), Ui.Dp(d));
            thumb.CornerRadius = Ui.Dp(r);
            thumb.Position = new Position(Ui.Dp(x - r), Ui.Dp(Height / 2 - r));
            var outer = r + PleyShape.ThumbRingOffset + PleyShape.ThumbRingStroke;
            ring.Size = new Size(Ui.Dp(outer * 2), Ui.Dp(outer * 2));
            ring.CornerRadius = Ui.Dp(outer);
            ring.Position = new Position(Ui.Dp(x - outer), Ui.Dp(Height / 2 - outer));
            if (focused) ring.Show();
            else ring.Hide();
        }
    }

    /// <summary>
    /// Play/Pause (design spec §2.7): a 64 dp circle of Fg at 15 % with a 32 dp glyph; focused, the
    /// accent with OnAccent ink, a little larger. It says Pause while the music plays or is about to.
    /// </summary>
    sealed class PlayPauseButton : View
    {
        const float Glyph = 32;

        readonly View circle;
        readonly ImageView glyph;
        bool pause;
        Animation? focusAnimation;

        public PlayPauseButton(Action onClick)
        {
            Size = new Size(Ui.Dp(PlaySize), Ui.Dp(PlaySize));
            Item = new FocusItem(MusicText.Play) { Selected = onClick };
            circle = new View
            {
                Size = new Size(Ui.Dp(PlaySize), Ui.Dp(PlaySize)),
                CornerRadius = Ui.Dp(PlaySize / 2),
                PivotPoint = global::Tizen.NUI.PivotPoint.Center,
            };
            glyph = Icons.Icon("play", Glyph, PleyColors.Fg);
            glyph.Position = new Position(Ui.Dp((PlaySize - Glyph) / 2), Ui.Dp((PlaySize - Glyph) / 2));
            circle.Add(glyph);
            Add(circle);
            Item.FocusChanged += Look;
            Look(false);
        }

        public FocusItem Item { get; }

        public void Set(bool showsPause)
        {
            if (showsPause == pause) return;
            pause = showsPause;
            glyph.ResourceUrl = Icons.Url(pause ? "pause" : "play");
        }

        void Look(bool focused)
        {
            if (Disposed) return;
            circle.BackgroundColor = focused ? PleyColors.Accent : new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.15f);
            glyph.Color = focused ? PleyColors.OnAccent : PleyColors.Fg;
            focusAnimation?.Stop();
            var scale = focused ? PleyShape.RoundScale : 1f;
            focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(circle, "Scale", new Vector3(scale, scale, 1));
            focusAnimation.Play();
        }
    }
}
