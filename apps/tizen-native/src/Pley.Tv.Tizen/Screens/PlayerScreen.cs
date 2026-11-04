using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading;
using Pley.Tv.Core;
using Pley.Tv.Core.Local;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;
using Timer = Tizen.NUI.Timer;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The player (design spec §2.7, TV addendum §5.2; the Android app's TvPlayerScreen) over the
/// Core's <see cref="PlayerModel"/>: the server's session on the TV's own player, on the video
/// plane under the window, with the subtitles drawn over it.
/// <list type="bullet">
/// <item><b>Controls hidden.</b> Only the video and subtitles show; Skip, Up Next and the sleep
/// timer's notice are all that appear unasked. OK shows the controls on the scrubber, as Up does (a
/// showing Skip takes it); Left and Right seek (<see cref="Seeker"/>), Down shows them on Play/Pause.
/// OK during or just after a seek confirms it and plays on.</item>
/// <item><b>Controls shown.</b> The title, the method badge and the clock at the top; the times,
/// the scrubber (with its chapters and the marker playing) and the control row at the bottom, over
/// a black scrim; focus moves as usual; OK on the scrubber plays and pauses, and Up on it goes to Skip
/// while it shows, else opens the chapters as a row of cards (<see cref="ChapterStrip"/>). The row's
/// buttons open the side panels
/// (<see cref="PlayerPanelSheet"/>). They go 5 s after the last key while it plays, unless paused,
/// scrubbing or in a panel.</item>
/// <item><b>Back</b> walks out: a panel, then Up Next, then the chapters, then the controls, then the player.</item>
/// <item><b>Media keys</b> work whatever shows; CAPTIONS turns the subtitles off and on.</item>
/// </list>
/// The model reports to the server through the app's reporter, so the last report goes out after
/// the screen closed. A playback the server stopped says why and leaves after 20 s; paused by the
/// sleep timer for 10 minutes, the player leaves.
/// </summary>
sealed class PlayerScreen : Sheet
{
    const float TimesTop = 399, TrackCentre = 425, RowTop = 441, TrackHeight = 4, Thumb = 10, ThumbFocused = 12, ChapterGap = 2;
    const float Button = 36, PlayButton = 44, TransportIcon = 24, PlayIcon = 26, TransportGap = 16;
    /// <summary>The seek preview's top, and Skip's, the sleep notice's and Up Next's bottom edges, controls hidden and shown.</summary>
    const float PreviewTop = 212, OverlayBottom = 489, OverlayBottomShown = 355;
    /// <summary>The spinner shows once playback has waited this long (the Android app's 500 ms).</summary>
    const long SpinnerAfterMs = 500;

    readonly Shell shell;
    readonly AppState state;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action closed;
    readonly SynchronizationContext? ui = SynchronizationContext.Current;
    readonly PlayerModel model;
    readonly Seeker seeker;
    readonly IDisposable keys;
    readonly MediaKeys mediaKeys;
    readonly List<IDisposable> watches = new List<IDisposable>();

    readonly View black;
    readonly View bars;
    readonly View scrim;
    readonly PictureSubtitleView pictures;
    readonly SubtitleView subtitles;
    readonly View spinner;
    readonly Animation spin;
    readonly TextLabel reconnecting;
    readonly View controls;
    readonly TextLabel title;
    readonly View banner;
    readonly TextLabel bannerTitle;
    readonly TextLabel bannerLine;
    /// <summary>The item the banner last showed for: another one beginning shows it again.</summary>
    long? bannerFor;
    bool bannerDue;
    /// <summary>The banner's height and how long it shows.</summary>
    const float BannerHeight = 120;
    const int BannerMs = 4000;
    readonly TextLabel line;
    readonly MethodBadge method;
    readonly TextLabel clock;
    readonly TextLabel ends;
    readonly TextLabel elapsed;
    readonly TextLabel remaining;
    readonly TextLabel badge;
    readonly TextLabel markerLabel;
    readonly View track;
    readonly List<(View Played, float From, float To)> segments = new List<(View, float, float)>();
    readonly View thumb;
    readonly View thumbRing;
    readonly SeekPreviewView preview;
    readonly ChapterStrip chapterStrip;
    /// <summary>The top of the chapters' row: its cards (6 dp in, for the focus ring) where the seek preview is.</summary>
    const float ChaptersTop = PreviewTop - 6;
    /// <summary>The start of the chapter focused in the cards: the scrubber and the preview show it.</summary>
    long? chapterAt;
    readonly FocusItem scrubber;
    readonly FocusGroup row = new FocusGroup(Axis.Row, "controls");
    readonly PlayerButton previous;
    readonly PlayerButton playPause;
    readonly PlayerButton next;
    readonly PlayerButton chapters;
    readonly PlayerButton sleep;
    readonly PlayerButton speed;
    readonly PlayerButton subtitlesButton;
    readonly PlayerButton audio;
    readonly PlayerButton quality;
    readonly PlayerButton info;
    readonly SkipButton skip;
    readonly SleepNoticeView notice;
    readonly UpNextView upNext;
    readonly ToastView toast;
    readonly Timer tick;

    PlayerPanelSheet? panel;
    View? error;
    FocusGroup? errorButtons;
    PlayerPhase? errorShown;
    RowOptions? rowOptions;
    IReadOnlyList<ChapterMark>? segmentsOf;
    long segmentsFor = -1;
    long? previewAt;
    /// <summary>The seek previews <see cref="previewAt"/> was shown with: ones that come mid-seek show at once.</summary>
    TrickplaySet? previewsOf;
    string? clockText;
    bool shown;
    bool gone;
    bool? wants;
    bool covered = true;
    /// <summary>After an error, black stays until the next session's first picture.</summary>
    bool awaitingFrame;
    long? lastFirstFrame;
    long waitingSince = -1;
    bool skipShown;
    bool noticeShown;
    bool upNextShown;
    bool stoppedLeaving;
    bool sleepLeaving;
    int sleepToken;
    int toastSerial;
    /// <summary>The subtitles the CAPTIONS key turned off, to turn back on.</summary>
    int? captionsRemembered;
    Animation? fade;
    Animation? scrimFade;
    Animation? coverFade;
    long hideAt;

    /// <param name="reporter">The app's reports queue: the last "stopped" goes out after the screen closed.</param>
    /// <param name="closed">The screen went (Home asks again for the resume points).</param>
    /// <param name="localFile">A video on a drive to play, as it is (<see cref="LocalPlayerSource"/>); null: the request's item, from the server.</param>
    public PlayerScreen(Shell shell, AppState state, PlayRequest request, IReporter reporter, Images images, KeyRepeatGate gate, Action closed, string? localFile = null) : base("player")
    {
        Covers = true;
        this.shell = shell;
        this.state = state;
        this.images = images;
        this.gate = gate;
        this.closed = closed;
        IPlayerSource source = new AppPlayerSource(state, () => Caps(state));
        if (localFile != null) source = new LocalPlayerSource(source, localFile);
        model = new PlayerModel(request, source, Engine, new ContextScheduler(), reporter);
        seeker = new Seeker(model.LivePositionMs, () => model.Clock.Value.DurationMs, SeekTo);
        // Asked for before the screen is made, which holds the UI half a second: the server answers
        // meanwhile. Its answer comes after (the UI thread's), and the watches below take the state as it is.
        var part = Profile.Start();
        Profile.Mark("player: asking");
        model.Start();
        Profile.Took("player screen: asked", part);
        part = Profile.Start();

        // Black until the first picture: the video plane under it shows nothing of the page meanwhile.
        black = new View { Size = Canvas(), BackgroundColor = Color.Black };
        View.Add(black);
        bars = new View { Size = Canvas() };
        View.Add(bars);
        // The controls' scrim fades with them, under the picture subtitles so they stay clear over it.
        scrim = Scrim();
        scrim.Opacity = 0;
        View.Add(scrim);
        // The title banner: what plays, at the controls' place over the top of their scrim, for a
        // few seconds as each item begins (the next episode playing by itself), the controls hidden.
        banner = new View { Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(BannerHeight)), Opacity = 0, Background = TopScrim() };
        bannerTitle = Text.Label("", PleyType.Title, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.SafeHorizontal - 300);
        bannerTitle.Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(PleyTv.SafeVertical));
        banner.Add(bannerTitle);
        bannerLine = Text.Label("", PleyType.Label, PleyColors.Muted, width: PleyTv.CanvasWidth - PleyTv.SafeHorizontal - 300);
        bannerLine.Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(PleyTv.SafeVertical + PleyType.Title.Line + 4));
        banner.Add(bannerLine);
        View.Add(banner);
        // Picture subtitles where their plane puts them, under the controls, as on the web app.
        pictures = new PictureSubtitleView();
        View.Add(pictures);
        controls = new View { Size = Canvas(), Opacity = 0 };
        View.Add(controls);
        // Over the controls' scrim, which mustn't dim them; above the controls while they show.
        subtitles = new SubtitleView(state.Settings.Value);
        View.Add(subtitles);
        title = Text.Label("", PleyType.Title, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.SafeHorizontal - 300);
        title.Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(PleyTv.SafeVertical));
        controls.Add(title);
        line = Text.Label("", PleyType.Label, PleyColors.Muted, width: PleyTv.CanvasWidth - PleyTv.SafeHorizontal - 300);
        line.Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(PleyTv.SafeVertical + PleyType.Title.Line + 4));
        controls.Add(line);
        clock = RightAligned(PleyType.Label, PleyColors.Muted, PleyTv.SafeVertical);
        controls.Add(clock);
        method = new MethodBadge();
        method.Hide();
        controls.Add(method);
        ends = RightAligned(PleyType.Label, PleyColors.Muted, PleyTv.SafeVertical + PleyType.Label.Line + 2);
        controls.Add(ends);

        var width = PleyTv.CanvasWidth - 2 * PleyTv.SafeHorizontal;
        elapsed = Text.Label("", PleyType.Caption, PleyColors.Fg, width: 200);
        elapsed.Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(TimesTop));
        controls.Add(elapsed);
        remaining = Text.Label("", PleyType.Caption, PleyColors.Fg, width: 200);
        remaining.HorizontalAlignment = HorizontalAlignment.End;
        remaining.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - 200), Ui.Dp(TimesTop));
        controls.Add(remaining);
        badge = Text.Label("", PleyType.Label, PleyColors.Fg, width: 120);
        badge.HorizontalAlignment = HorizontalAlignment.Center;
        badge.PositionY = Ui.Dp(TimesTop - PleyType.Label.Line - 6);
        controls.Add(badge);
        markerLabel = Text.Label("", PleyType.Caption, PleyColors.Muted, width: 160);
        markerLabel.HorizontalAlignment = HorizontalAlignment.Center;
        markerLabel.PositionY = Ui.Dp(TrackCentre + 6);
        markerLabel.Hide();
        controls.Add(markerLabel);

        // The track: a segment between each chapter's start, each with its played part.
        track = new View
        {
            Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(TrackCentre - TrackHeight / 2)),
            Size = new Size(Ui.Dp(width), Ui.Dp(TrackHeight)),
        };
        controls.Add(track);
        var ring = ThumbFocused + 2 * PleyShape.ThumbRingOffset + 2 * PleyShape.ThumbRingStroke;
        thumbRing = new View
        {
            Size = new Size(Ui.Dp(ring), Ui.Dp(ring)),
            CornerRadius = Ui.Dp(ring / 2),
            BorderlineWidth = Ui.Dp(PleyShape.ThumbRingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1,
            BackgroundColor = Color.Transparent,
            Opacity = 0,
        };
        controls.Add(thumbRing);
        thumb = new View { Size = new Size(Ui.Dp(Thumb), Ui.Dp(Thumb)), CornerRadius = Ui.Dp(Thumb / 2), BackgroundColor = PleyColors.Fg };
        controls.Add(thumb);
        chapterStrip = new ChapterStrip(images, model.Preview, GoToChapter, CloseChapters);
        chapterStrip.PositionY = Ui.Dp(ChaptersTop);
        controls.Add(chapterStrip);
        preview = new SeekPreviewView(images);
        preview.Hide();
        controls.Add(preview);

        Profile.Took("player screen: its picture layers, banner and scrubber", part);
        part = Profile.Start();
        // The control row: Chapters · Sleep timer · Speed at the left; Previous · Play/Pause ·
        // Next centred; Subtitles · Audio · Quality · Info at the right.
        chapters = new PlayerButton(PlayerText.Chapters, "list-video", Button, PleyTv.IconSize, OpenChapters);
        sleep = new PlayerButton(PlayerText.Sleep, "moon", Button, PleyTv.IconSize, () => OpenPanel(PlayerPanelKind.Sleep));
        speed = new PlayerButton(PlayerText.Speed, "fast-forward", Button, PleyTv.IconSize, () => OpenPanel(PlayerPanelKind.Speed));
        previous = new PlayerButton(PlayerText.Previous, "skip-back", Button, TransportIcon, () => model.Previous(seeker.Target ?? model.LivePositionMs()));
        playPause = new PlayerButton(PlayerText.Pause, "pause", PlayButton, PlayIcon, PlayPause);
        next = new PlayerButton(PlayerText.Next, "skip-forward", Button, TransportIcon, () => model.Next(seeker.Target ?? model.LivePositionMs()));
        subtitlesButton = new PlayerButton(PlayerText.Subtitles, "captions", Button, PleyTv.IconSize, () => OpenPanel(PlayerPanelKind.Subtitles));
        audio = new PlayerButton(PlayerText.Audio, "audio-lines", Button, PleyTv.IconSize, () => OpenPanel(PlayerPanelKind.Audio));
        quality = new PlayerButton(PlayerText.Quality, "gauge", Button, PleyTv.IconSize, () => OpenPanel(PlayerPanelKind.Quality));
        info = new PlayerButton(PlayerText.Info, "info", Button, PleyTv.IconSize, () => OpenPanel(PlayerPanelKind.Info));
        foreach (var b in RowButtons)
        {
            b.Hide();
            controls.Add(b);
        }

        skip = new SkipButton(model.Skip) { Opacity = 0 };
        skip.Item.Shown = false;
        View.Add(skip);
        notice = new SleepNoticeView(model.KeepWatching) { Opacity = 0 };
        notice.Keep.Shown = false;
        View.Add(notice);
        upNext = new UpNextView(images, state.Settings.Value.ReduceMotion, model.PlayNext, model.CancelUpNext) { Opacity = 0 };
        foreach (var item in upNext.Group.Children.OfType<FocusItem>()) item.Shown = false;
        View.Add(upNext);

        Profile.Took("player screen: its control row, Skip, Up Next", part);
        part = Profile.Start();
        spinner = Spinner();
        spinner.Position = new Position(Ui.Dp(PleyTv.CanvasWidth / 2 - 20), Ui.Dp(PleyTv.CanvasHeight / 2 - 20));
        spinner.Hide();
        View.Add(spinner);
        spin = new Animation(1000) { Looping = true };
        spin.AnimateBy(spinner, "Orientation", new Rotation(new Radian(new Degree(360)), PositionAxis.Z));
        // "Reconnecting…" 16 dp under the ring, on the seek badge's backing: the picture behind it can be anything.
        reconnecting = Text.Label(PlayerText.Reconnecting, PleyType.Label, PleyColors.Fg);
        reconnecting.HorizontalAlignment = HorizontalAlignment.Center;
        var reconnectingWidth = reconnecting.NaturalSize.Width / Ui.Scale + 24;
        reconnecting.Size = new Size(Ui.Dp(reconnectingWidth), Ui.Dp(PleyType.Label.Line + 12));
        reconnecting.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - reconnectingWidth) / 2), Ui.Dp(PleyTv.CanvasHeight / 2 + 20 + 16));
        reconnecting.BackgroundColor = new Color(PleyColors.Bg.R, PleyColors.Bg.G, PleyColors.Bg.B, 0.6f);
        reconnecting.CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        reconnecting.Hide();
        View.Add(reconnecting);
        toast = new ToastView();
        toast.Hide();
        View.Add(toast);

        // Focus, top to bottom: Skip, the sleep notice, the scrubber, the row, Up Next.
        scrubber = new FocusItem("scrubber")
        {
            Selected = Ok,
            OwnKey = (key, down) =>
            {
                switch (key)
                {
                    case "Left":
                    case "Right":
                        if (down && !Busy) Step(key == "Left" ? -1 : 1);
                        return true;
                    case "Up":
                        // While a seek waits for OK, the chapters as cards over the scrubber, as YouTube's
                        // TV app has them; else Up moves on, to Skip Intro while it shows.
                        if (seeker.Target == null || model.State.Value.Chapters.Count == 0) return false;
                        if (down) OpenChapters();
                        return true;
                    case "Down":
                        if (down) Tree.Focus(playPause.Item);
                        return true;
                    default:
                        return false;
                }
            },
        };
        scrubber.FocusChanged += on =>
        {
            if (Disposed) return;
            thumbRing.Opacity = on ? 1 : 0;
            thumb.Size = new Size(Ui.Dp(on ? ThumbFocused : Thumb), Ui.Dp(on ? ThumbFocused : Thumb));
            thumb.CornerRadius = Ui.Dp((on ? ThumbFocused : Thumb) / 2);
            Layout();
        };
        Root.Add(skip.Item);
        Root.Add(notice.Group);
        Root.Add(scrubber);
        Root.Add(row);
        Root.Add(upNext.Group);

        keys = shell.Input.Push(OnKey);
        // The media keys, from the window or the TV's media key service, are the player's whatever
        // shows over it; pushed after the other keys, they come first.
        mediaKeys = new MediaKeys(shell.Input, () => !gone, OnMedia);
        gate.HoldEnded += Released;
        shell.Shown += AppShown;
        model.Toast += ShowToast;
        Back = OnBack;
        Gone += Ended;

        Profile.Took("player screen: spinner, focus and keys", part);
        part = Profile.Start();
        tick = new Timer(250);
        tick.Tick += (_, _) =>
        {
            Tick();
            return !gone;
        };
        tick.Start();
        watches.Add(model.State.Watch(Render));
        watches.Add(model.Cues.Watch(subtitles.Show));
        watches.Add(model.Picture.Watch(pictures.Show));
        watches.Add(state.Settings.Watch(subtitles.Restyle));
        Profile.Took("player screen: its first render", part);
    }

    bool Disposed => gone;

    /// <summary>Starting or between sessions: Left, Right and OK wait, as on Android.</summary>
    bool Busy => model.State.Value.Phase.Kind == PhaseKind.Preparing;

    IEnumerable<PlayerButton> RowButtons => new[] { chapters, sleep, speed, previous, playPause, next, subtitlesButton, audio, quality, info };

    static Size Canvas() => new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));

    static TextLabel RightAligned(TextStyle style, Color color, float top)
    {
        var label = Text.Label("", style, color, width: 240);
        label.HorizontalAlignment = HorizontalAlignment.End;
        label.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - 240), Ui.Dp(top));
        return label;
    }

    /// <summary>What the TV says it plays, with Settings' passthrough choices; null when it can't say.</summary>
    static IReadOnlyDictionary<string, object?>? Caps(AppState state)
    {
        try
        {
            return TizenCaps.Of(Platform.Facts(), state.Settings.Value.Passthrough);
        }
        catch (Exception)
        {
            // The server decides alone, as for a client that sends no report.
            return null;
        }
    }

    /// <summary>The TV's player for the video: on the video plane, its picture's size telling where black bars go.</summary>
    IVideoEngine Engine()
    {
        var engine = new TizenMediaEngine(video: true);
        engine.VideoSize += Bars;
        return engine;
    }

    /// <summary>Shows the player, started as it was made: the item's details, a session, its stream.</summary>
    public void Show() => shell.Push(this);

    // ---- what the playback says

    void Render(PlayerState s)
    {
        if (gone) return;
        Text.Show(title, s.Title);
        Text.Show(line, s.Subtitle ?? "");
        Banner(s);
        if (s.Method != null) method.Set(s.Method);
        method.SetVisible(s.Method != null);
        PlaceMethod();
        var wantsToPlay = s.Phase.WantsToPlay;
        if (wants != wantsToPlay)
        {
            wants = wantsToPlay;
            playPause.Set(wantsToPlay ? PlayerText.Pause : PlayerText.Play, wantsToPlay ? "pause" : "play");
        }
        BuildRow(RowOptions.Of(s));
        var isError = s.Phase.Kind == PhaseKind.Error;
        if (lastFirstFrame == null && s.FirstFrameMs != null) awaitingFrame = false;
        lastFirstFrame = s.FirstFrameMs;
        if (isError) awaitingFrame = true;
        Cover(s.FirstFrameMs == null || awaitingFrame);
        var waiting = !isError && s.Phase.Kind is PhaseKind.Preparing or PhaseKind.Buffering;
        if (!waiting) waitingSince = -1;
        else if (waitingSince < 0) waitingSince = Environment.TickCount64;
        Waiting();
        if (isError)
        {
            if (errorShown != s.Phase) ShowError(s.Phase);
        }
        else if (error != null) HideError();
        UpNext(s.UpNext);
        if (s.Trickplay != previewsOf)
        {
            previewsOf = s.Trickplay;
            previewAt = null;
            chapterStrip.PreviewsCame();
        }
        Overlays();
        // The server stopped the playback: the error says why, then the player leaves.
        if (s.StoppedByServer && !stoppedLeaving)
        {
            stoppedLeaving = true;
            After(PlayerText.StoppedLeaveMs, Close);
        }
        // Paused by the sleep timer for 10 minutes: the player leaves.
        var asleep = s.Sleep.Fired && s.Phase.Kind == PhaseKind.Paused;
        if (asleep && !sleepLeaving)
        {
            sleepLeaving = true;
            var token = ++sleepToken;
            After(SleepTimer.LeaveAfterMs, () =>
            {
                if (token == sleepToken) Close();
            });
        }
        else if (!asleep && sleepLeaving)
        {
            sleepLeaving = false;
            sleepToken++;
        }
        // The end, with nothing next: back to where playing started.
        if (s.Phase.Kind == PhaseKind.Ended) OnUi(Close);
        if (shown) Layout();
    }

    /// <summary>Black over the video plane: until the first picture, and after an error until the next one.</summary>
    void Cover(bool on)
    {
        if (covered == on) return;
        covered = on;
        coverFade?.Stop();
        if (on)
        {
            black.Opacity = 1;
            return;
        }
        coverFade = Fade.To(black, false, PleyMotion.OverlayMs);
    }

    /// <summary>The spinner, once playback has waited 500 ms; "Reconnecting…" under it while the server is out of reach.</summary>
    void Waiting()
    {
        var on = waitingSince >= 0 && Environment.TickCount64 - waitingSince >= SpinnerAfterMs;
        if (on && !spinner.Visibility)
        {
            spinner.Show();
            spin.Play();
        }
        else if (!on && spinner.Visibility)
        {
            spinner.Hide();
            spin.Stop();
        }
        if (on && model.State.Value.Reconnecting) reconnecting.Show();
        else reconnecting.Hide();
    }

    /// <summary>
    /// The control row for <paramref name="options"/> (the Android app's TvControlRow): Chapters
    /// when the file has them, the sleep timer and Speed (where the TV's player has other speeds)
    /// from the left safe edge; Previous and Next around the transport when an episode or chapters
    /// have them, centred; Subtitles, Audio and Quality when there is a choice, and Info, to the
    /// right safe edge. Focus goes along them all; it stays on the button it was on, else Play/Pause.
    /// </summary>
    void BuildRow(RowOptions options)
    {
        if (options == rowOptions) return;
        rowOptions = options;
        var focused = Tree.Current;
        var wasInRow = focused != null && focused.Parent == row;
        row.Clear();
        foreach (var b in RowButtons) b.Hide();
        var left = new List<PlayerButton>();
        if (options.Chapters) left.Add(chapters);
        left.Add(sleep);
        if (options.Speed) left.Add(speed);
        var centre = new List<PlayerButton>();
        if (options.PreviousNext) centre.Add(previous);
        centre.Add(playPause);
        if (options.PreviousNext) centre.Add(next);
        var right = new List<PlayerButton>();
        if (options.Subtitles) right.Add(subtitlesButton);
        if (options.Audio) right.Add(audio);
        if (options.Quality) right.Add(quality);
        right.Add(info);
        void Lay(List<PlayerButton> buttons, float x, float gap)
        {
            foreach (var b in buttons)
            {
                // The smaller ones' centres in line with Play/Pause's.
                b.Position = new Position(Ui.Dp(x), Ui.Dp(RowTop + (PlayButton - b.SizeDp) / 2));
                x += b.SizeDp + gap;
                b.Show();
                row.Add(b.Item);
            }
        }
        static float Width(List<PlayerButton> buttons, float gap) => buttons.Sum(b => b.SizeDp) + gap * (buttons.Count - 1);
        Lay(left, PleyTv.SafeHorizontal, PleyTv.ButtonGap);
        Lay(centre, (PleyTv.CanvasWidth - Width(centre, TransportGap)) / 2, TransportGap);
        Lay(right, PleyTv.CanvasWidth - PleyTv.SafeHorizontal - Width(right, PleyTv.ButtonGap), PleyTv.ButtonGap);
        if (wasInRow && focused!.Parent != row) Tree.Focus(playPause.Item);
        else if (!wasInRow) row.RememberChild(row.Children.ToList().IndexOf(playPause.Item));
    }

    /// <summary>A side panel over the controls, which stay while it shows; focus comes back to its button when it closes.</summary>
    void OpenPanel(PlayerPanelKind kind)
    {
        if (gone || panel != null) return;
        var sheet = new PlayerPanelSheet(shell, model, kind);
        panel = sheet;
        sheet.Gone += () =>
        {
            panel = null;
            if (!gone) Poke();
        };
        sheet.Show();
    }

    void ClosePanel() => panel?.Close();

    /// <summary>
    /// Skip and the sleep notice, as the state says. Skip shows while its marker runs, unless Up
    /// Next, an error or a seek preview is on screen, and takes focus while the controls are hidden;
    /// the notice takes it as Skip does, Skip first.
    /// </summary>
    void Overlays()
    {
        var s = model.State.Value;
        var free = error == null && s.UpNext == null;
        // Over the scrubber with the controls up, as on Android TV, but not over a seek's preview or the chapters.
        var skipNow = s.SkipLabel != null && free && !(shown && (seeker.Target != null || chapterStrip.Open));
        if (skipNow) skip.SetLabel(s.SkipLabel!);
        var noticeNow = s.Sleep.SecondsLeft != null && error == null;
        if (s.Sleep.SecondsLeft is int left) notice.Set(left);
        if (skipNow != skipShown)
        {
            skipShown = skipNow;
            skip.Item.Shown = skipNow;
            Fade.To(skip, skipNow);
            if (skipNow && !shown && shell.Top == this && free) Tree.Focus(skip.Item);
            else if (!skipNow && Tree.Current == skip.Item) Refocus();
        }
        if (noticeNow != noticeShown)
        {
            noticeShown = noticeNow;
            notice.Keep.Shown = noticeNow;
            Fade.To(notice, noticeNow);
            if (noticeNow && !skipShown && !shown && shell.Top == this && free) Tree.Focus(notice.Keep);
            else if (!noticeNow && Tree.Current == notice.Keep) Refocus();
        }
        PlaceOverlays();
    }

    /// <summary>Focus where it goes when what had it went: the scrubber with the controls shown, else Skip or the notice, else nowhere.</summary>
    void Refocus()
    {
        if (shown) Tree.Focus(scrubber);
        else if (skipShown) Tree.Focus(skip.Item);
        else if (noticeShown) Tree.Focus(notice.Keep);
        else Tree.Blur();
    }

    /// <summary>Up Next takes the screen's attention: the controls make way, Play now has focus; Back cancels it.</summary>
    void UpNext(UpNextCard? card)
    {
        if (card != null)
        {
            upNext.Set(card);
            if (upNextShown) return;
            upNextShown = true;
            seeker.Cancel();
            ClosePanel();
            ShowControls(false);
            foreach (var item in upNext.Group.Children.OfType<FocusItem>()) item.Shown = true;
            Fade.To(upNext, true);
            PlaceOverlays();
            Tree.Focus(upNext.PlayNow);
        }
        else if (upNextShown)
        {
            upNextShown = false;
            var had = Tree.Current?.Parent == upNext.Group;
            foreach (var item in upNext.Group.Children.OfType<FocusItem>()) item.Shown = false;
            Fade.To(upNext, false);
            if (had) Refocus();
        }
    }

    /// <summary>Skip, the notice and Up Next at the bottom; Skip and the notice above the controls while they show; the toast under the clock then.</summary>
    void PlaceOverlays()
    {
        var bottom = shown ? OverlayBottomShown : OverlayBottom;
        var right = PleyTv.CanvasWidth - PleyTv.SafeHorizontal;
        skip.Position = new Position(Ui.Dp(right - skip.WidthDp), Ui.Dp(bottom - 40));
        notice.Position = new Position(Ui.Dp(PleyTv.SafeHorizontal), Ui.Dp(bottom - notice.HeightDp));
        upNext.Position = new Position(Ui.Dp(right - UpNextView.Width), Ui.Dp(OverlayBottom - upNext.HeightDp));
        toast.Position = new Position(Ui.Dp(right - toast.WidthDp), Ui.Dp(PleyTv.SafeVertical + (shown ? 52 : 0)));
    }

    /// <summary>The method badge, 12 dp left of the clock.</summary>
    void PlaceMethod()
    {
        var clockWidth = clock.NaturalSize.Width / Ui.Scale;
        method.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - clockWidth - 12 - method.WidthDp),
            Ui.Dp(PleyTv.SafeVertical) + (Text.LineHeight(PleyType.Label) - method.SizeHeight) / 2);
    }

    void ShowToast(PlayerToast t)
    {
        if (gone) return;
        var serial = ++toastSerial;
        toast.Set(t);
        PlaceOverlays();
        toast.Show();
        toast.RaiseToTop();
        After(t.Ms, () =>
        {
            if (serial == toastSerial) toast.Hide();
        });
    }

    /// <summary>
    /// Black where the letterboxed picture isn't: the window is clear over the whole video plane,
    /// and the TV leaves the plane's bars empty, so without these the TV's own screens show there.
    /// The bars go round <see cref="PictureArea"/>'s rectangle, where the engine has the TV put the
    /// picture, and reach a pixel into it, so no seam shows between them whatever the film's shape.
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
        // The rectangle the engine told the TV to put the picture in.
        var (sw, sh) = PictureArea.Screen;
        var area = PictureArea.Of(videoWidth, videoHeight, sw, sh);
        float w = sw, h = sh, left = area.X, top = area.Y;
        const float into = 1;
        void Bar(float x, float y, float bw, float bh)
        {
            if (bw >= 1 && bh >= 1) bars.Add(new View { Position = new Position(x, y), Size = new Size(bw, bh), BackgroundColor = Color.Black });
        }
        float bottom = area.Y + area.Height, right = area.X + area.Width;
        if (top >= 1 || bottom <= h - 1)
        {
            Bar(0, 0, w, top + into);
            Bar(0, bottom - into, w, h - bottom + into);
        }
        if (left >= 1 || right <= w - 1)
        {
            Bar(0, 0, left + into, h);
            Bar(right - into, 0, w - right + into, h);
        }
    }

    void Tick()
    {
        if (gone) return;
        var now = Environment.TickCount64;
        // The controls go 5 s after the last key, unless paused, scrubbing or in a panel.
        if (shown && panel == null && model.State.Value.Phase.WantsToPlay && hideAt > 0 && now >= hideAt && seeker.Target == null && !chapterStrip.Open) ShowControls(false);
        Waiting();
        if (shown) Layout();
    }

    /// <summary>The times, the played part, the thumb, the marker playing and the seek preview, at the target while a seek is coming.</summary>
    void Layout()
    {
        var s = model.State.Value;
        var duration = model.Clock.Value.DurationMs;
        var position = model.LivePositionMs();
        var at = chapterAt ?? seeker.Target ?? position;
        elapsed.Text = PlayerText.Time(at);
        remaining.Text = PlayerText.Remaining(at, duration);
        var time = DateTime.Now.ToString("t", CultureInfo.CurrentCulture);
        if (time != clockText)
        {
            clockText = time;
            clock.Text = time;
            PlaceMethod();
        }
        ends.Text = s.Phase.Kind == PhaseKind.Paused
            ? PlayerText.EndsAt(DateTime.Now.AddMilliseconds(Math.Max(0, duration - position)).ToString("t", CultureInfo.CurrentCulture))
            : "";
        if (!ReferenceEquals(s.Chapters, segmentsOf) || duration != segmentsFor) Segments(s.Chapters, duration);
        var width = track.SizeWidth;
        float X(long ms) => duration > 0 ? (float)(width * Math.Max(0, Math.Min(1, ms / (double)duration))) : 0;
        // Blue up to the thumb: where it plays, or where a seek or a chapter focused would take it.
        var playedTo = X(at);
        foreach (var (part, from, to) in segments) part.SizeWidth = Math.Max(0, Math.Min(playedTo, to) - from);
        var cx = track.PositionX + X(at);
        var cy = Ui.Dp(TrackCentre);
        thumb.Position = new Position(cx - thumb.SizeWidth / 2, cy - thumb.SizeHeight / 2);
        thumbRing.Position = new Position(cx - thumbRing.SizeWidth / 2, cy - thumbRing.SizeHeight / 2);
        // Not over the seek preview, whose time and chapter say where it goes: Android shows no
        // badge with the controls up, and here it sat on the preview's chapter line.
        if (seeker.BadgeAt is long b && !(shown && seeker.Target != null))
        {
            badge.Text = PlayerText.Badge(b);
            badge.PositionX = Math.Max(track.PositionX, Math.Min(track.PositionX + width - badge.SizeWidth, cx - badge.SizeWidth / 2));
            badge.Show();
        }
        else badge.Hide();
        // "Intro" or "Credits" under the span of the marker playing.
        if (Markers.Active(s.Markers, position) is { } marker && duration > 0)
        {
            Text.Show(markerLabel, Markers.Name(marker));
            var mid = track.PositionX + X((marker.StartMs + marker.EndMs) / 2);
            markerLabel.PositionX = Math.Max(track.PositionX, Math.Min(track.PositionX + width - markerLabel.SizeWidth, mid - markerLabel.SizeWidth / 2));
            markerLabel.Show();
        }
        else markerLabel.Hide();
        Preview(seeker.Target, duration);
        Overlays();
    }

    /// <summary>The track's segments, a 2 dp gap at each chapter's start.</summary>
    void Segments(IReadOnlyList<ChapterMark> chapters, long duration)
    {
        segmentsOf = chapters;
        segmentsFor = duration;
        foreach (var child in track.Children.ToList())
        {
            track.Remove(child);
            child.Dispose();
        }
        segments.Clear();
        var w = track.SizeWidth;
        var gap = Ui.Dp(ChapterGap);
        var edges = new List<float> { 0 };
        if (duration > 0)
            edges.AddRange(chapters.Select(c => (float)(w * c.StartMs / (double)duration)).Where(x => x > gap && x < w - gap));
        edges.Add(w);
        var h = Ui.Dp(TrackHeight);
        for (var i = 0; i < edges.Count - 1; i++)
        {
            var a = edges[i] + (i == 0 ? 0 : gap / 2);
            var b = edges[i + 1] - (i == edges.Count - 2 ? 0 : gap / 2);
            if (b <= a) continue;
            var part = new View
            {
                Position = new Position(a, 0),
                Size = new Size(b - a, h),
                CornerRadius = h / 2,
                BackgroundColor = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.25f),
                ClippingMode = ClippingModeType.ClipChildren,
            };
            var played = new View { Size = new Size(0, h), CornerRadius = h / 2, BackgroundColor = PleyColors.Accent };
            part.Add(played);
            track.Add(part);
            segments.Add((played, a, b));
        }
    }

    /// <summary>
    /// The seek preview while a seek is coming and the controls show: centred on the target, kept
    /// in the safe area; asked for every 2 s of the target at most.
    /// </summary>
    void Preview(long? target, long duration)
    {
        if (!shown || chapterStrip.Open || target is not long t)
        {
            previewAt = null;
            preview.Hide();
            return;
        }
        var bucket = t / Trickplay.StepMs * Trickplay.StepMs;
        if (bucket != previewAt)
        {
            previewAt = bucket;
            var chapters = model.State.Value.Chapters;
            var index = PlayerText.ChapterAt(chapters, t);
            var chapter = index is int i ? chapters[i] : null;
            var frame = model.Preview(bucket);
            preview.Set(t, frame, frame == null ? chapter?.ImageUrl : null, index is int n ? PlayerText.ChapterLine(chapters[n], n) : null);
        }
        var fraction = duration > 0 ? Math.Max(0, Math.Min(1, t / (double)duration)) : 0;
        var centre = PleyTv.SafeHorizontal + (float)((PleyTv.CanvasWidth - 2 * PleyTv.SafeHorizontal) * fraction);
        var x = Math.Max(PleyTv.SafeHorizontal, Math.Min(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - SeekPreviewView.PictureWidth, centre - SeekPreviewView.PictureWidth / 2));
        preview.Position = new Position(Ui.Dp(x), Ui.Dp(preview.HasPicture ? PreviewTop : PreviewTop + SeekPreviewView.PictureHeight + 6));
        preview.Show();
    }

    // ---- keys

    /// <summary>
    /// The keys before focus has them: media keys and CAPTIONS whatever shows; with the controls
    /// hidden, OK, the arrows and the keys that show the controls. Shown, focus has the keys (Back is the sheet's).
    /// </summary>
    bool OnKey(string key, bool down)
    {
        if (gone) return false;
        if (RemoteKeys.Captions(key))
        {
            if (down && error == null) captionsRemembered = model.ToggleCaptions(captionsRemembered);
            return true;
        }
        if (shell.Top != this || error != null) return false;
        // Up Next: its two buttons, nothing above or below them.
        if (model.State.Value.UpNext != null) return key is "Up" or "Down";
        if (shown)
        {
            Poke();
            return false;
        }
        if (RemoteKeys.Info(key) || RemoteKeys.Menu(key) || RemoteKeys.Guide(key))
        {
            if (down) ShowControls(true, skipShown ? skip.Item : scrubber);
            return true;
        }
        switch (key)
        {
            case "Return":
            case "KP_Enter":
                // Skip or Keep watching has focus: OK is theirs.
                if (Tree.Current != null) return false;
                // On the way up, as OK selects elsewhere: the controls, on the scrubber, as Up shows them;
                // OK on the scrubber plays and pauses. Just after a seek, it confirms it.
                if (!down)
                {
                    if (seeker.Target != null) Ok();
                    else ShowControls(true, scrubber);
                }
                return true;
            case "Left":
            case "Right":
                if (down && !Busy)
                {
                    ShowControls(true, scrubber);
                    Step(key == "Left" ? -1 : 1);
                }
                return true;
            case "Up":
                if (down) ShowControls(true, scrubber);
                return true;
            case "Down":
                if (down) ShowControls(true, playPause.Item);
                return true;
        }
        return false;
    }

    /// <summary>A media key, from the window or the media key service: Play, Pause, fast forward and rewind (30 s), Next and Previous, Stop.</summary>
    void OnMedia(MediaAction action)
    {
        // An error on screen: only Stop, which leaves.
        if (error != null && action != MediaAction.Stop) return;
        switch (action)
        {
            case MediaAction.Play:
                model.Play();
                break;
            case MediaAction.Pause:
                model.Pause();
                if (!shown && shell.Top == this && model.State.Value.UpNext == null) ShowControls(true, scrubber);
                break;
            case MediaAction.PlayPause:
                PlayPause();
                break;
            case MediaAction.Rewind:
                Jump(-3);
                break;
            case MediaAction.Forward:
                Jump(3);
                break;
            case MediaAction.Next:
                model.Next(seeker.Target ?? model.LivePositionMs());
                break;
            case MediaAction.Previous:
                model.Previous(seeker.Target ?? model.LivePositionMs());
                break;
            case MediaAction.Stop:
                Close();
                break;
        }
    }

    /// <summary>
    /// OK on the video or the scrubber: during or just after a seek it goes there, playing (paused,
    /// it plays from there); else it plays or pauses.
    /// </summary>
    void Ok()
    {
        if (seeker.Confirm())
        {
            if (!model.WantsToPlay) model.Play();
            Layout();
            return;
        }
        PlayPause();
    }

    void PlayPause()
    {
        if (Busy) return;
        if (model.WantsToPlay)
        {
            model.Pause();
            // Paused, the controls stay; playing again, they go after a while.
            if (shell.Top == this && model.State.Value.UpNext == null) ShowControls(true, shown ? null : scrubber);
        }
        else
        {
            model.Play();
            if (shown) Poke();
        }
        Layout();
    }

    void Step(int direction)
    {
        Poke();
        if (gate.Holding) seeker.Repeat(direction, Environment.TickCount64);
        else
        {
            seeker.Press(direction);
            Settle(Seeker.WaitMs);
        }
        Layout();
    }

    /// <summary>Fast forward and rewind: 30 s, three of Left and Right's steps. They seek without waiting for OK.</summary>
    void Jump(int steps)
    {
        if (Busy) return;
        seeker.Press(Math.Sign(steps), Seeker.StepMs * Math.Abs(steps));
        Settle(Seeker.SettleMs);
        if (shown) Poke();
        Layout();
    }

    /// <summary>Presses in a row seek once they stop, <paramref name="waitMs"/> after the last (OK sooner); the badge goes a little later.</summary>
    void Settle(long waitMs)
    {
        var press = seeker.Presses;
        After(waitMs, () =>
        {
            seeker.Settle(press);
            Layout();
        });
        After(Seeker.BadgeMs, () =>
        {
            seeker.HideBadge(press);
            Layout();
        });
    }

    void Released()
    {
        if (gone) return;
        if (!seeker.Release()) return;
        // Let go, the scrub waits for OK where it got to, as presses do.
        Settle(Seeker.WaitMs);
        Layout();
    }

    void SeekTo(long ms)
    {
        // A while after a seek, OK plays and pauses again.
        var number = seeker.Seeks + 1;
        After(Seeker.ConfirmMs, () => seeker.Settled(number));
        model.SeekTo(ms);
    }

    /// <summary>
    /// The chapters as cards over the scrubber, on the chapter where the scrubber is, in the seek
    /// preview's place and size: from a waiting seek (Up) or the Chapters button. The video plays
    /// on while they show; as focus goes through them, the scrubber's thumb shows the chapter's start.
    /// </summary>
    void OpenChapters()
    {
        if (model.State.Value.Chapters.Count == 0) return;
        // The seek gives way to the chapters, which start where it was going: chosen, one goes there;
        // closed, it plays on from where it is.
        var from = seeker.Target ?? model.LivePositionMs();
        seeker.Cancel();
        chaptersFrom = Tree.Current;
        chapterStrip.Focused = start =>
        {
            chapterAt = start;
            Layout();
        };
        chapterStrip.Show(model.State.Value.Chapters, from);
        if (chapterStrip.Item.Parent == null) Root.Add(chapterStrip.Item);
        Tree.Focus(chapterStrip.Item);
        Poke();
    }

    /// <summary>Where focus was as the chapters opened: the scrubber, or the Chapters button.</summary>
    FocusItem? chaptersFrom;

    /// <summary>The cards go; focus back where they were opened from.</summary>
    void CloseChapters()
    {
        if (!chapterStrip.Open) return;
        chapterAt = null;
        chapterStrip.Hide();
        // Its pictures go while the film plays on: the TV's decoder shares the memory.
        chapterStrip.Forget();
        if (chapterStrip.Item.Parent != null) Root.Remove(chapterStrip.Item);
        if (shown && !(chaptersFrom != null && Tree.Focus(chaptersFrom))) Tree.Focus(scrubber);
        chaptersFrom = null;
        Layout();
    }

    /// <summary>A chapter chosen: it goes to its start, and the cards go.</summary>
    void GoToChapter(long startMs)
    {
        seeker.Cancel();
        model.SeekTo(startMs);
        CloseChapters();
    }

    /// <summary>Shows or hides the controls; shown, focus goes to <paramref name="focus"/> (where it was, when null and already shown).</summary>
    void ShowControls(bool on, FocusItem? focus = null)
    {
        if (!on) CloseChapters();
        // The controls have the title: the banner makes way.
        if (on && banner.Opacity > 0) Fade.To(banner, false, PleyMotion.OverlayMs);
        if (on)
        {
            if (focus != null && Tree.Current != focus) Tree.Focus(focus);
            else if (Tree.Current == null) Tree.Focus(scrubber);
            Poke();
        }
        if (shown != on)
        {
            shown = on;
            fade?.Stop();
            fade = Fade.To(controls, on, PleyMotion.OverlayMs);
            scrimFade?.Stop();
            scrimFade = Fade.To(scrim, on, PleyMotion.OverlayMs);
            subtitles.Lift(on);
            Layout();
        }
        if (!on) Refocus();
    }

    /// <summary>A key came: the controls stay a while longer.</summary>
    void Poke() => hideAt = Environment.TickCount64 + PlayerText.HideAfterMs;

    bool OnBack()
    {
        if (error == null && model.State.Value.UpNext != null)
        {
            model.CancelUpNext();
            return true;
        }
        if (error == null && chapterStrip.Open)
        {
            CloseChapters();
            return true;
        }
        // A seek waiting for OK: Back lets it go, and it plays on from where it was.
        if (error == null && shown && seeker.Target != null)
        {
            seeker.Cancel();
            Poke();
            Layout();
            return true;
        }
        if (error == null && shown)
        {
            ShowControls(false);
            return true;
        }
        Close();
        return true;
    }

    /// <summary>The app went to the background: it pauses, as the TV takes its player back.</summary>
    void AppShown(bool visible)
    {
        if (!gone && !visible) model.Pause();
    }

    // ---- errors

    /// <summary>
    /// A playback that stopped (design spec §8): its heading, why, and "Try again" (focused) when it
    /// may help, with Back; else Back alone. Centred on black.
    /// </summary>
    void ShowError(PlayerPhase phase)
    {
        HideError();
        errorShown = phase;
        seeker.Cancel();
        ClosePanel();
        var view = new View { Size = Canvas(), BackgroundColor = Color.Black };
        var width = PleyTv.CanvasWidth - 320;
        var heading = Text.Label(phase.Title ?? PlayerText.ErrorTitle, PleyType.PageTitle, PleyColors.Fg, width: width);
        heading.HorizontalAlignment = HorizontalAlignment.Center;
        var message = Text.Label(phase.Message ?? "", PleyType.Body, PleyColors.Muted, maxLines: 4, width: width);
        message.HorizontalAlignment = HorizontalAlignment.Center;
        message.VerticalAlignment = VerticalAlignment.Top;
        var messageHeight = Math.Min(Math.Max(message.GetHeightForWidth(Ui.Dp(width)), Text.LineHeight(PleyType.Body)), Text.LineHeight(PleyType.Body) * 4) / Ui.Scale;
        message.SizeHeight = Ui.Dp(messageHeight);
        var buttons = new ButtonRow(12);
        ButtonView first;
        if (phase.CanRetry)
        {
            first = buttons.Add(new ButtonView(PlayerText.TryAgain, ButtonKind.Pill, model.Retry, iconName: "refresh-cw"));
            buttons.Add(new ButtonView(PlayerText.BackLabel, ButtonKind.Text, Close));
        }
        else first = buttons.Add(new ButtonView(PlayerText.BackLabel, ButtonKind.Pill, Close, iconName: "arrow-left"));
        var block = PleyType.PageTitle.Line + 12 + messageHeight + 32 + PleyTv.ButtonHeight;
        var top = (PleyTv.CanvasHeight - block) / 2;
        heading.Position = new Position(Ui.Dp(160), Ui.Dp(top));
        message.Position = new Position(Ui.Dp(160), Ui.Dp(top + PleyType.PageTitle.Line + 12));
        buttons.Position = new Position(Ui.Dp((PleyTv.CanvasWidth - buttons.WidthDp) / 2), Ui.Dp(top + PleyType.PageTitle.Line + 12 + messageHeight + 32));
        view.Add(heading);
        view.Add(message);
        view.Add(buttons);
        View.Add(view);
        toast.RaiseToTop();
        error = view;
        // Up and Down stay on the buttons: the controls under them are hidden.
        buttons.Group.Exit = d => d is Direction.Up or Direction.Down;
        errorButtons = buttons.Group;
        Root.Add(buttons.Group);
        ShowControls(false);
        Tree.Focus(first.Item);
    }

    void HideError()
    {
        errorShown = null;
        if (errorButtons != null)
        {
            if (Tree.Current?.Parent == errorButtons) Tree.Blur();
            Root.Remove(errorButtons);
            errorButtons = null;
        }
        if (error == null) return;
        error.Unparent();
        error.Dispose();
        error = null;
    }

    // ---- leaving

    void Close()
    {
        if (gone) return;
        ClosePanel();
        shell.Pop(this);
    }

    /// <summary>Off the screen: the model reports where it stopped and ends the session; the TV's player is let go.</summary>
    void Ended()
    {
        if (gone) return;
        gone = true;
        tick.Stop();
        StopTimers();
        spin.Stop();
        keys.Dispose();
        mediaKeys.Dispose();
        gate.HoldEnded -= Released;
        shell.Shown -= AppShown;
        model.Toast -= ShowToast;
        foreach (var w in watches) w.Dispose();
        watches.Clear();
        model.Close();
        // The pictures' textures go now, not when the view is collected.
        pictures.Show(null);
        chapterStrip.Forget();
        preview.Forget();
        closed();
    }

    void OnUi(Action action)
    {
        if (ui == null) action();
        else ui.Post(_ =>
        {
            if (!gone) action();
        }, null);
    }

    /// <summary>Runs <paramref name="action"/> after <paramref name="ms"/>, unless the player has gone by then.</summary>
    void After(long ms, Action action)
    {
        foreach (var done in spent) done.Dispose();
        spent.Clear();
        var timer = new Timer((uint)Math.Max(1, ms));
        pending.Add(timer);
        timer.Tick += (_, _) =>
        {
            // Not disposed in its own tick: let go of on the next After, or as the player goes.
            pending.Remove(timer);
            spent.Add(timer);
            if (!gone) action();
            return false;
        };
        timer.Start();
    }

    /// <summary>
    /// Timers waiting to fire, held here: held by nothing, one could be collected while the TV
    /// still had it, and the app died when it fired (a seek waits 5 s for OK).
    /// </summary>
    readonly List<Timer> pending = new List<Timer>();

    /// <summary>Timers that have fired, disposed later.</summary>
    readonly List<Timer> spent = new List<Timer>();

    void StopTimers()
    {
        foreach (var t in pending.Concat(spent))
        {
            t.Stop();
            t.Dispose();
        }
        pending.Clear();
        spent.Clear();
    }

    /// <summary>A 40 dp arc in the accent, turning while the stream comes.</summary>
    static View Spinner()
    {
        var arc = new ArcVisual { Thickness = Ui.Dp(3), StartAngle = 0, SweepAngle = 270, Cap = ArcVisual.CapType.Round, MixColor = PleyColors.Accent };
        return new View { Size = new Size(Ui.Dp(40), Ui.Dp(40)), Background = arc.OutputVisualMap };
    }

    /// <summary>The black scrim behind the controls: 65 % at the top, 25 % through the middle, 90 % at the bottom.</summary>
    /// <summary>The banner's shade: black at the controls' scrim's top (65 %) to clear at its foot.</summary>
    static PropertyMap TopScrim()
    {
        var offsets = new PropertyArray();
        var colors = new PropertyArray();
        foreach (var (at, alpha) in new[] { (0f, 0.65f), (1f, 0f) })
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
    /// The banner, as an item begins playing (the next episode by itself, or the first): what plays,
    /// for a few seconds, unless the controls show, which have it already.
    /// </summary>
    void Banner(PlayerState s)
    {
        if (s.ItemId != bannerFor)
        {
            bannerFor = s.ItemId;
            bannerDue = true;
        }
        if (!bannerDue || s.Phase.Kind != PhaseKind.Playing) return;
        bannerDue = false;
        if (shown) return;
        Text.Show(bannerTitle, s.Title);
        Text.Show(bannerLine, s.Subtitle ?? "");
        Fade.To(banner, true, PleyMotion.OverlayMs);
        var item = s.ItemId;
        After(BannerMs, () =>
        {
            if (bannerFor == item) Fade.To(banner, false, PleyMotion.OverlayMs);
        });
    }

    static View Scrim()
    {
        var offsets = new PropertyArray();
        var colors = new PropertyArray();
        foreach (var (at, alpha) in new[] { (0f, 0.65f), (0.22f, 0.25f), (0.55f, 0.25f), (0.78f, 0.7f), (1f, 0.9f) })
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
        return new View { Size = Canvas(), Background = map };
    }
}
