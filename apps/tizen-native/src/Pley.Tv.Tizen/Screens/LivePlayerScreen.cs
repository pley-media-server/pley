using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Live;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The live player (the Android app's TvLivePlayerScreen): the channel full screen on the TV's
/// own player, under a clear window. With nothing over it, Up and Down and Channel up and down
/// change channel (Up the next), digits tune a number; OK shows the controls (the channel list,
/// the favourite) over what's on; Menu or Guide opens the channel list; Info shows what's on for
/// 5 s; Back hides what shows, then leaves. Live TV is watched as it airs: play, pause, rewind,
/// fast forward, Left and Right do nothing. A channel that doesn't start says why, with Try again
/// (or Watch here instead, every connection being in use), Other channels and Back to the guide.
/// The app hidden, the channel and its connection are let go; shown again, it tunes again.
/// </summary>
sealed class LivePlayerScreen : Sheet
{
    const int TickMs = 250;

    readonly Shell shell;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action closed;
    readonly LivePlayerModel model;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly IDisposable keys;
    readonly Timer tick;

    readonly View black;
    readonly View bars;
    readonly View stage = new View();
    readonly View infoBar = new View();
    readonly View digits;
    readonly TextLabel digitsText;
    readonly View spinner;
    readonly Animation spin;
    readonly FocusGroup controls = new FocusGroup(Axis.Row, "controls");
    readonly FocusGroup trouble = new FocusGroup(Axis.Column, "trouble");
    readonly Dictionary<(long, float), ChannelLogoView> logos = new Dictionary<(long, float), ChannelLogoView>();
    readonly List<Animation> spins = new List<Animation>();

    bool controlsShown;
    bool infoShown;
    long infoUntil;
    long hideAt;
    bool listOpen;
    string? stageShown;
    string? barShown;
    LiveTrouble? troubleShown;
    long? channelShown;
    bool gone;

    public LivePlayerScreen(Shell shell, AppLiveSource source, LiveWatch watch, Images images, KeyRepeatGate gate, Action closed) : base("live-player")
    {
        Covers = true;
        this.shell = shell;
        this.images = images;
        this.gate = gate;
        this.closed = closed;

        black = new View { Size = Canvas(), BackgroundColor = Color.Black };
        View.Add(black);
        bars = new View { Size = Canvas() };
        View.Add(bars);
        View.Add(stage);
        View.Add(infoBar);
        (spinner, spin) = LiveViews.Spinner(36);
        spinner.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - 36) / 2), Ui.Dp((PleyTv.CanvasHeight - 36) / 2));
        spinner.Hide();
        View.Add(spinner);
        digits = new View { CornerRadius = Ui.Dp(PleyShape.DialogRadius), BackgroundColor = new Color(0, 0, 0, 0.85f) };
        digitsText = Text.Label("", PleyType.Code, PleyColors.Fg);
        digitsText.Position = new Position(Ui.Dp(24), Ui.Dp(12));
        digits.Add(digitsText);
        digits.Hide();
        View.Add(digits);
        Root.Add(controls);
        Root.Add(trouble);

        var scheduler = new ContextScheduler();
        model = new LivePlayerModel(source, new LivePlayback(source, Engine, scheduler), scheduler, watch.ChannelId, watch.CategoryKey, watch.Seen);
        watches.Add(model.State.Watch(_ => Draw()));
        watches.Add(model.Channel.Watch(_ => Draw()));
        watches.Add(model.Now.Watch(_ => Draw()));
        watches.Add(model.Category.Watch(_ => Draw()));
        watches.Add(model.Digits.Watch(Digits));
#if PROFILE
        // From tuning to the picture: the engine's marks fall between these.
        watches.Add(model.State.Watch(s => Profile.Mark($"live: {s.Is.GetType().Name}{(s.Buffering ? ", stalled" : "")}{(s.Is is LivePhase.Tuning ? $" {model.Channel.Value?.Number} {model.Channel.Value?.Name}" : "")}")));
#endif

        keys = shell.Input.Push(OnKey);
        shell.Shown += Shown;
        Back = OnBack;
        Gone += Ended;
        tick = new Timer(TickMs);
        tick.Tick += (_, _) =>
        {
            Tick();
            return !gone;
        };
        tick.Start();
    }

    static Size Canvas() => new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));

    public void Show() => shell.Push(this);

    /// <summary>The TV's player for a channel: on the video plane, its picture's size telling where black bars go.</summary>
    IMediaEngine Engine()
    {
        var engine = new TizenMediaEngine(video: true);
        engine.VideoSize += Bars;
        return engine;
    }

    LiveState State => model.State.Value;

    // ---- keys

    bool OnKey(string key, bool down)
    {
        if (gone || shell.Top != this) return false;
        if (controlsShown) Poke();
        if (RemoteKeys.ChannelUp(key) || RemoteKeys.ChannelDown(key))
        {
            if (down) model.Step(RemoteKeys.ChannelUp(key) ? 1 : -1);
            return true;
        }
        if (RemoteKeys.Digit(key) is { } d)
        {
            if (down) model.Digit(d);
            return true;
        }
        if (RemoteKeys.Guide(key) || RemoteKeys.Menu(key))
        {
            if (!down) OpenList();
            return true;
        }
        if (RemoteKeys.Info(key))
        {
            if (!down) Peek();
            return true;
        }
        // No pause, no going back on live.
        if (RemoteKeys.Transport(key)) return true;
        if (controlsShown || State.Is is LivePhase.Failed) return false;
        switch (key)
        {
            case "Up":
                if (down) model.Step(1);
                return true;
            case "Down":
                if (down) model.Step(-1);
                return true;
            case "Left":
            case "Right":
                return true;
            case "Return":
            case "KP_Enter":
                // On the way up, as OK selects elsewhere.
                if (!down) ShowControls(true);
                return true;
        }
        return false;
    }

    bool OnBack()
    {
        if (controlsShown)
        {
            ShowControls(false);
            return true;
        }
        shell.Pop(this);
        return true;
    }

    /// <summary>What's on, for 5 s once it plays.</summary>
    void Peek()
    {
        infoShown = true;
        infoUntil = 0;
        Draw();
    }

    void ShowControls(bool on)
    {
        if (controlsShown == on) return;
        controlsShown = on;
        if (on) Poke();
        barShown = null;
        Draw();
    }

    /// <summary>A key came: the controls stay a while longer.</summary>
    void Poke() => hideAt = Environment.TickCount64 + LivePlayerModel.ControlsMs;

    void Tick()
    {
        if (gone) return;
        var now = Environment.TickCount64;
        var playing = State.Playing;
        if (controlsShown && playing && hideAt > 0 && now >= hideAt) ShowControls(false);
        if (infoShown && playing)
        {
            if (infoUntil == 0) infoUntil = now + LivePlayerModel.InfoMs;
            else if (now >= infoUntil)
            {
                infoShown = false;
                Draw();
            }
        }
    }

    void OpenList()
    {
        if (listOpen || gone) return;
        listOpen = true;
        ShowControls(false);
        model.OpenList();
        var list = new LiveChannelListSheet(shell, model, images, (channel, from) => model.Tune(channel.Id, channel, from));
        list.Gone += () =>
        {
            listOpen = false;
            if (!gone) Draw();
        };
        list.Show();
        Draw();
    }

    void Shown(bool shown)
    {
        if (gone) return;
        if (shown) model.Resume();
        else model.Stop();
    }

    // ---- drawing

    void Digits(ChannelDigits d)
    {
        if (gone) return;
        if (d.Text is not { } text)
        {
            digits.Hide();
            return;
        }
        var style = d.Typed.Length > 0 ? PleyType.Code : PleyType.Section;
        digitsText.PixelSize = Ui.Dp(style.Size);
        digitsText.FontFamily = style.Family;
        digitsText.FontStyle = Text.Weight(style.Weight);
        digitsText.Text = text;
        var w = digitsText.NaturalSize.Width;
        var h = Text.LineHeight(style);
        digitsText.Size = new Size(w + 2, h);
        digits.Size = new Size(w + Ui.Dp(48), h + Ui.Dp(24));
        digits.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal) - digits.SizeWidth, Ui.Dp(PleyTv.SafeVertical));
        digits.Show();
        digits.RaiseToTop();
    }

    /// <summary>Everything over the picture, as the state says: tuning, sound only or trouble; the spinner; what's on.</summary>
    void Draw()
    {
        if (gone) return;
        var state = State;
        var channel = model.Channel.Value;
        var now = model.Now.Value;
        if (state.ChannelId != channelShown)
        {
            // Another channel: what's on put away, the last one's bars gone.
            channelShown = state.ChannelId;
            infoShown = false;
            Bars(0, 0);
            foreach (var logo in logos.Values) logo.Dispose();
            logos.Clear();
        }
        var playing = state.Playing;
        // Clear only once pictures come: the engine says it plays when Start returns, before the
        // first frame, and the window is clear here, so while it buffers what is under the app
        // (the TV's menu) would show round the spinner. A stall goes to black with the spinner.
        black.Opacity = playing && !state.Buffering && !state.AudioOnly ? 0 : 1;
        if (state.Buffering && playing) Spin(spinner, spin, true);
        else Spin(spinner, spin, false);

        var name = channel != null ? LiveRules.ChannelName(channel, model.Category.Value) : "";
        var stageKey = state.Is switch
        {
            LivePhase.Failed f => "failed",
            LivePhase.Playing => state.AudioOnly && channel != null ? $"radio|{channel.Id}|{channel.OnAt(now)?.Title}" : "",
            _ => channel != null ? $"tuning|{channel.Id}|{name}|{channel.Number}|{channel.OnAt(now)?.Title}" : "",
        };
        if (state.Is is LivePhase.Failed failed)
        {
            if (!ReferenceEquals(failed.Trouble, troubleShown) || stageShown != "failed")
            {
                troubleShown = failed.Trouble;
                stageShown = "failed";
                Clear(stage);
                ShowControls(false);
                Trouble(failed.Trouble, channel);
            }
        }
        else if (stageKey != stageShown)
        {
            stageShown = stageKey;
            troubleShown = null;
            var hadTrouble = trouble.Children.Count > 0;
            trouble.Clear();
            Clear(stage);
            if (hadTrouble && !controlsShown) Tree.Blur();
            if (stageKey.StartsWith("tuning", StringComparison.Ordinal)) Tuning(channel!, name, now);
            else if (stageKey.StartsWith("radio", StringComparison.Ordinal)) SoundOnly(channel!, now);
        }

        var barVisible = channel != null && playing && !listOpen && (controlsShown || infoShown);
        if (!barVisible)
        {
            // By what's drawn, not barShown: ShowControls(false) forgets that to have the bar drawn
            // again, and the bar stayed on screen with nothing to take it away (Back then left).
            if (infoBar.Children.Count > 0 || controls.Children.Count > 0)
            {
                barShown = null;
                // Before the buttons go: after, the focused one has no parent to tell it by.
                if (Tree.Current?.Parent == controls) Tree.Blur();
                Clear(infoBar);
                controls.Clear();
            }
            if (controlsShown && !playing) controlsShown = false;
            return;
        }
        var barKey = $"{channel!.GetHashCode()}|{name}|{now / 60}|{controlsShown}|{playing}";
        if (barKey == barShown) return;
        // Drawn again (the favourite changed, a minute passed): focus stays on the same button.
        var focusedAt = Tree.Current != null && Tree.Current.Parent == controls ? controls.Children.ToList().IndexOf(Tree.Current) : 0;
        barShown = barKey;
        Clear(infoBar);
        controls.Clear();
        InfoBar(channel, name, now);
        if (controlsShown && controls.Children.Count > 0)
            Tree.Focus((FocusItem)controls.Children[Math.Max(0, Math.Min(focusedAt, controls.Children.Count - 1))]);
    }

    void Spin(View view, Animation animation, bool on)
    {
        if (on)
        {
            view.Show();
            animation.Play();
        }
        else
        {
            view.Hide();
            animation.Stop();
        }
    }

    /// <summary>Takes the views out of <paramref name="container"/>, keeping the logos for the next drawing.</summary>
    void Clear(View container)
    {
        foreach (var logo in logos.Values)
            if (IsIn(logo, container)) logo.Unparent();
        foreach (var child in container.Children.ToList())
        {
            container.Remove(child);
            child.Dispose();
        }
        foreach (var s in spins) s.Stop();
        spins.Clear();
    }

    static bool IsIn(View view, View container)
    {
        for (var p = view.GetParent() as View; p != null; p = p.GetParent() as View)
            if (p == container) return true;
        return false;
    }

    ChannelLogoView Logo(LiveChannel channel, float w, float h, TextStyle? style = null, Color? well = null, float radius = PleyShape.TileRadius)
    {
        if (!logos.TryGetValue((channel.Id, w), out var logo))
        {
            logo = new ChannelLogoView(channel, model.Origin, images, w, h, name: true, style: style, well: well, radiusDp: radius);
            logos[(channel.Id, w)] = logo;
        }
        logo.Unparent();
        return logo;
    }

    /// <summary>The channel's logo, name and what's on while it is tuned, on black.</summary>
    void Tuning(LiveChannel channel, string name, long now)
    {
        var logo = Logo(channel, 200, 112, PleyType.Title, LiveViews.Alpha(Color.White, 0.06f), PleyShape.DialogRadius);
        var block = 112 + 22 + 36 + 18 + PleyType.Section.Line + 6 + PleyType.Body.Line;
        var top = (PleyTv.CanvasHeight - block) / 2;
        logo.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - 200) / 2), Ui.Dp(top));
        stage.Add(logo);
        var (ring, turn) = LiveViews.Spinner(36);
        ring.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - 36) / 2), Ui.Dp(top + 112 + 22));
        stage.Add(ring);
        turn.Play();
        spins.Add(turn);
        var line = Centred(LiveText.NumberAndName(channel, name), PleyType.Section, PleyColors.Fg, top + 112 + 22 + 36 + 18);
        stage.Add(line);
        stage.Add(Centred(channel.OnAt(now)?.Title ?? LiveText.Tuning, PleyType.Body, PleyColors.Muted, top + 112 + 22 + 36 + 18 + PleyType.Section.Line + 6));
    }

    /// <summary>A channel with sound only (radio): its logo where the picture would be, with what's on.</summary>
    void SoundOnly(LiveChannel channel, long now)
    {
        var logo = Logo(channel, 240, 135, PleyType.Title, LiveViews.Alpha(Color.White, 0.06f), PleyShape.DialogRadius);
        var block = 135 + 18 + 20 + 8 + PleyType.Section.Line;
        var top = (PleyTv.CanvasHeight - block) / 2;
        logo.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - 240) / 2), Ui.Dp(top));
        stage.Add(logo);
        var label = LiveViews.Fitted(LiveText.Radio, PleyType.Label, PleyColors.Muted, 300);
        var w = 20 + 8 + label.SizeWidth / Ui.Scale;
        var x = (PleyTv.CanvasWidth - w) / 2;
        var icon = Icons.Icon("radio", 20, PleyColors.Muted);
        icon.Position = new Position(Ui.Dp(x), Ui.Dp(top + 135 + 18));
        stage.Add(icon);
        label.Position = new Position(Ui.Dp(x + 28), Ui.Dp(top + 135 + 18 + (20 - PleyType.Label.Line) / 2));
        stage.Add(label);
        if (channel.OnAt(now) is { } on) stage.Add(Centred(on.Title, PleyType.Section, PleyColors.Fg, top + 135 + 18 + 20 + 8));
    }

    static TextLabel Centred(string text, TextStyle style, Color color, float top, float width = 600)
    {
        var label = Text.Label(text, style, color, width: width);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        label.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - width) / 2), Ui.Dp(top));
        return label;
    }

    /// <summary>
    /// The bar at the foot: the channel's logo, number and name, what's on with its times, how far
    /// it is and what's next; Live; then the controls (the channel list first, the favourite) or
    /// the hint.
    /// </summary>
    void InfoBar(LiveChannel channel, string name, long now)
    {
        var clock = LiveClock.Local;
        var on = channel.OnAt(now);
        var next = channel.NextAt(now);
        const float rowHeight = 88, padTop = 64;
        var below = controlsShown ? 16 + PleyTv.ButtonHeight + 6 + PleyType.Caption.Line : 12 + PleyType.Caption.Line;
        var height = padTop + rowHeight + below + PleyTv.SafeVertical;
        var top = PleyTv.CanvasHeight - height;
        var scrim = new View
        {
            Position = new Position(0, Ui.Dp(top)),
            Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(height)),
            Background = Scrim(),
        };
        infoBar.Add(scrim);
        var left = PleyTv.SafeHorizontal;
        var y = top + padTop;
        var logo = Logo(channel, 112, 63, well: LiveViews.Alpha(Color.White, 0.08f));
        logo.Position = new Position(Ui.Dp(left), Ui.Dp(y + (rowHeight - 63) / 2));
        infoBar.Add(logo);
        var x = left + 112 + 18;
        var live = State.Playing ? LiveBadge() : null;
        var right = PleyTv.CanvasWidth - PleyTv.SafeHorizontal - (live != null ? live.SizeWidth / Ui.Scale + 16 : 0);
        var textWidth = right - x;

        // The number and name, a star for a favourite.
        var lineX = x;
        if (channel.Number is { } number)
        {
            var n = LiveViews.Fitted(number, PleyType.Label, PleyColors.Muted, 80);
            n.Position = new Position(Ui.Dp(lineX), Ui.Dp(y));
            infoBar.Add(n);
            lineX += n.SizeWidth / Ui.Scale + 8;
        }
        var nameLabel = LiveViews.Fitted(name, PleyType.Label, PleyColors.Muted, Math.Max(40, right - lineX - 20));
        nameLabel.Position = new Position(Ui.Dp(lineX), Ui.Dp(y));
        infoBar.Add(nameLabel);
        if (channel.Favorite)
        {
            var star = Icons.Icon("star", 14, PleyColors.Accent);
            star.Position = new Position(Ui.Dp(lineX + 6) + nameLabel.SizeWidth, Ui.Dp(y + (PleyType.Label.Line - 14) / 2));
            infoBar.Add(star);
        }
        y += PleyType.Label.Line + 2;

        // What's on, with its badge.
        var titleX = x;
        if (on != null && LiveViews.BadgeOf(on) is { } badge)
        {
            badge.Position = new Position(Ui.Dp(titleX), Ui.Dp(y) + (Text.LineHeight(PleyType.Section) - badge.SizeHeight) / 2);
            infoBar.Add(badge);
            titleX += badge.SizeWidth / Ui.Scale + 8;
        }
        var title = Text.Label(on?.Title ?? (channel.HasGuide ? LiveText.NothingNow : LiveText.NoGuide), PleyType.Section, PleyColors.Fg, width: right - titleX);
        title.Position = new Position(Ui.Dp(titleX), Ui.Dp(y));
        infoBar.Add(title);
        y += PleyType.Section.Line;
        if (on != null)
        {
            var span = LiveViews.Fitted($"{clock.Span(on)} · {LiveText.TimeLeft(on, now)}", PleyType.Caption, PleyColors.Muted, textWidth - 200);
            span.Position = new Position(Ui.Dp(x), Ui.Dp(y + 6));
            infoBar.Add(span);
            var bar = LiveViews.Progress(on, now, 180, LiveViews.Alpha(Color.White, 0.18f));
            bar.Position = new Position(Ui.Dp(x + 12) + span.SizeWidth, Ui.Dp(y + 6 + (PleyType.Caption.Line - 3) / 2));
            infoBar.Add(bar);
            y += 6 + PleyType.Caption.Line;
        }
        if (next != null)
        {
            var nextLabel = Text.Label(clock.Next(next), PleyType.Caption, PleyColors.Subtle, width: textWidth);
            nextLabel.Position = new Position(Ui.Dp(x), Ui.Dp(y + 4));
            infoBar.Add(nextLabel);
        }
        if (live != null)
        {
            live.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal) - live.SizeWidth, Ui.Dp(top + padTop + (rowHeight - 25) / 2));
            infoBar.Add(live);
        }

        var under = top + padTop + rowHeight;
        if (!controlsShown)
        {
            var hint = Text.Label(LiveText.Hint, PleyType.Caption, PleyColors.Subtle, width: 600);
            hint.Position = new Position(Ui.Dp(left), Ui.Dp(under + 12));
            infoBar.Add(hint);
            return;
        }
        var list = new RoundButton(LiveText.PlayerChannels, "list", OpenList);
        list.Position = new Position(Ui.Dp(left), Ui.Dp(under + 16));
        infoBar.Add(list);
        controls.Add(list.Item);
        var favorite = new RoundButton(channel.Favorite ? LiveText.FavoriteRemove : LiveText.FavoriteAdd, "star", () =>
        {
            Poke();
            model.ToggleFavorite();
        });
        favorite.Position = new Position(Ui.Dp(left + PleyTv.ButtonHeight + PleyTv.ButtonGap), Ui.Dp(under + 16));
        infoBar.Add(favorite);
        controls.Add(favorite.Item);
    }

    /// <summary>Live, red with a dot.</summary>
    static View LiveBadge()
    {
        var label = LiveViews.Fitted(LiveText.Live.ToUpperInvariant(), PleyType.Badge, Color.White, 80);
        var width = 12 + 7 + 7 + label.SizeWidth / Ui.Scale + 12;
        var pill = new View
        {
            Size = new Size(Ui.Dp(width), Ui.Dp(25)),
            CornerRadius = Ui.Dp(12.5f),
            BackgroundColor = PleyColors.Danger,
        };
        pill.Add(new View
        {
            Position = new Position(Ui.Dp(12), Ui.Dp(9)),
            Size = new Size(Ui.Dp(7), Ui.Dp(7)),
            CornerRadius = Ui.Dp(3.5f),
            BackgroundColor = Color.White,
        });
        label.Position = new Position(Ui.Dp(26), Ui.Dp(25) / 2 - label.SizeHeight / 2);
        pill.Add(label);
        return pill;
    }

    /// <summary>Black from clear at the top to 75 %, then 92 % at the foot.</summary>
    static PropertyMap Scrim()
    {
        var offsets = new PropertyArray();
        var colors = new PropertyArray();
        foreach (var (at, alpha) in new[] { (0f, 0f), (0.5f, 0.75f), (1f, 0.92f) })
        {
            offsets.Add(new PropertyValue(at));
            colors.Add(new PropertyValue(new Color(0, 0, 0, alpha)));
        }
        var map = new PropertyMap();
        map.Add(Visual.Property.Type, new PropertyValue((int)Visual.Type.Gradient));
        map.Add(GradientVisualProperty.StartPosition, new PropertyValue(new Vector2(0, -0.5f)));
        map.Add(GradientVisualProperty.EndPosition, new PropertyValue(new Vector2(0, 0.5f)));
        map.Add(GradientVisualProperty.StopOffset, new PropertyValue(offsets));
        map.Add(GradientVisualProperty.StopColor, new PropertyValue(colors));
        return map;
    }

    /// <summary>
    /// Why the channel doesn't play, with Try again, Other channels and Back to the guide. Every
    /// connection in use, what holds each, with Watch here instead where it may be taken over,
    /// focused first.
    /// </summary>
    void Trouble(LiveTrouble t, LiveChannel? channel)
    {
        trouble.Clear();
        stage.Add(new View { Size = Canvas(), BackgroundColor = new Color(0, 0, 0, 0.88f) });
        const float width = 520, pad = 32;
        var inner = width - 2 * pad;
        var card = new View
        {
            CornerRadius = Ui.Dp(PleyShape.DialogRadius),
            BackgroundColor = PleyColors.Surface2,
            BorderlineWidth = Ui.Dp(1),
            BorderlineColor = PleyColors.Surface4,
            BorderlineOffset = -1,
        };
        float y = pad;
        if (channel != null)
        {
            var logo = Logo(channel, 120, 68, well: LiveViews.Alpha(Color.White, 0.06f));
            logo.Position = new Position(Ui.Dp((width - 120) / 2), Ui.Dp(y));
            card.Add(logo);
            y += 68;
        }
        else
        {
            var icon = Icons.Icon(LiveViews.Icon("triangle-alert"), 40, PleyColors.Accent);
            icon.Position = new Position(Ui.Dp((width - 40) / 2), Ui.Dp(y));
            card.Add(icon);
            y += 40;
        }
        y += 18;
        y = CentredIn(card, LiveText.TroubleTitle(t, channel?.Name ?? ""), PleyType.Section, PleyColors.Fg, y, inner, pad, 2);
        var connections = t.Busy;
        var firstToTake = connections.FirstOrDefault(c => c.CanTakeOver);
        FocusItem? first = null;
        if (connections.Count > 0)
        {
            y = CentredIn(card, LiveText.ConnectionsWatching(connections.Count), PleyType.Body, PleyColors.Muted, y + 10, inner, pad, 2) + 14;
            foreach (var c in connections)
            {
                var rowHeight = 10 + PleyType.Label.Line + PleyType.Caption.Line + 10;
                var row = new View
                {
                    Position = new Position(Ui.Dp(pad), Ui.Dp(y)),
                    Size = new Size(Ui.Dp(inner), Ui.Dp(rowHeight)),
                    CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                    BackgroundColor = LiveViews.Alpha(Color.White, 0.04f),
                };
                var textWidth = inner - 32;
                if (c.CanTakeOver)
                {
                    var channelId = c.ChannelId;
                    var take = new ButtonView(LiveText.TakeOver, ButtonKind.Pill, () => model.TakeOver(channelId));
                    take.Position = new Position(Ui.Dp(inner - 16 - take.WidthDp), Ui.Dp((rowHeight - PleyTv.ButtonHeight) / 2));
                    row.Add(take);
                    trouble.Add(take.Item);
                    if (c == firstToTake) first = take.Item;
                    textWidth -= take.WidthDp + 12;
                }
                var held = Text.Label(c.ChannelName, PleyType.Label, PleyColors.Fg, width: textWidth);
                held.Position = new Position(Ui.Dp(16), Ui.Dp(10));
                row.Add(held);
                var who = Text.Label(LiveText.Holders(c), PleyType.Caption, PleyColors.Muted, width: textWidth);
                who.Position = new Position(Ui.Dp(16), Ui.Dp(10 + PleyType.Label.Line));
                row.Add(who);
                card.Add(row);
                y += rowHeight + 8;
            }
            if (firstToTake == null) y = CentredIn(card, LiveText.NoneToTake, PleyType.Caption, PleyColors.Subtle, y + 6, inner, pad, 2);
        }
        else if (!string.IsNullOrWhiteSpace(t.Message))
        {
            y = CentredIn(card, t.Message, PleyType.Body, PleyColors.Muted, y + 10, inner, pad, 4);
        }
        y += 24;
        var buttons = new ButtonRow();
        ButtonView? retry = null;
        if (t.CanRetry) retry = buttons.Add(new ButtonView(LiveText.PlayerRetry, ButtonKind.Pill, model.Retry, iconName: "refresh-cw"));
        var others = buttons.Add(new ButtonView(LiveText.OtherChannels, ButtonKind.Pill, OpenList, iconName: "list"));
        buttons.Add(new ButtonView(LiveText.BackToGuide, ButtonKind.Pill, () => shell.Pop(this)));
        var cardWidth = Math.Max(width, buttons.WidthDp + 2 * pad);
        buttons.Position = new Position(Ui.Dp((cardWidth - buttons.WidthDp) / 2), Ui.Dp(y));
        card.Add(buttons);
        trouble.Add(buttons.Group);
        y += PleyTv.ButtonHeight + pad;
        if (cardWidth > width)
        {
            // The buttons need more room than the card: everything else moves to the middle with them.
            foreach (var child in card.Children)
                if (child != buttons) child.PositionX += Ui.Dp((cardWidth - width) / 2);
        }
        card.Size = new Size(Ui.Dp(cardWidth), Ui.Dp(y));
        card.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - cardWidth) / 2), Ui.Dp(Math.Max(PleyTv.SafeVertical, (PleyTv.CanvasHeight - y) / 2)));
        stage.Add(card);
        Tree.Focus(first ?? retry?.Item ?? others.Item);
    }

    /// <summary>A centred line or a few in the card, from <paramref name="top"/>: where the next goes.</summary>
    static float CentredIn(View card, string text, TextStyle style, Color color, float top, float width, float left, int maxLines)
    {
        var label = Text.Label(text, style, color, maxLines: maxLines, width: width);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        label.VerticalAlignment = VerticalAlignment.Top;
        var h = Math.Min(label.GetHeightForWidth(Ui.Dp(width)), Text.LineHeight(style) * maxLines);
        label.SizeHeight = h;
        label.Position = new Position(Ui.Dp(left), Ui.Dp(top));
        card.Add(label);
        return top + h / Ui.Scale;
    }

    /// <summary>
    /// Black where the letterboxed picture isn't: the window is clear over the whole video plane,
    /// and the TV leaves the plane's bars empty. Each bar reaches a pixel into the picture, so no
    /// seam shows between them.
    /// </summary>
    void Bars(int videoWidth, int videoHeight)
    {
        if (gone) return;
        foreach (var bar in bars.Children.ToList())
        {
            bars.Remove(bar);
            bar.Dispose();
        }
        if (videoWidth <= 0 || videoHeight <= 0) return;
        var w = Ui.Dp(PleyTv.CanvasWidth);
        var h = Ui.Dp(PleyTv.CanvasHeight);
        var scale = Math.Min(w / videoWidth, h / videoHeight);
        var left = (float)Math.Floor((w - videoWidth * scale) / 2);
        var top = (float)Math.Floor((h - videoHeight * scale) / 2);
        void Bar(float x, float y, float bw, float bh)
        {
            if (bw >= 1 && bh >= 1) bars.Add(new View { Position = new Position(x, y), Size = new Size(bw, bh), BackgroundColor = Color.Black });
        }
        if (top >= 1)
        {
            Bar(0, 0, w, top + 1);
            Bar(0, h - top - 1, w, top + 1);
        }
        if (left >= 1)
        {
            Bar(0, 0, left + 1, h);
            Bar(w - left - 1, 0, left + 1, h);
        }
    }

    /// <summary>Off the screen: watching ends, the server is told, the TV's player let go.</summary>
    void Ended()
    {
        if (gone) return;
        gone = true;
        tick.Stop();
        spin.Stop();
        foreach (var s in spins) s.Stop();
        keys.Dispose();
        shell.Shown -= Shown;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        model.Dispose();
        closed();
    }
}
