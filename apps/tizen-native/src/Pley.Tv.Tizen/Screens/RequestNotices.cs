using System;
using System.Collections.Generic;
using Pley.Tv.Core;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Requests' news over the pages, one at a time (the Android app's TvRequestNotices): on a TV, what
/// became ready to watch. A card at the bottom right with the title's poster, the news, Open and
/// Close. It shows once the remote has rested 1.5 s, the app is in front, and the shell is on top
/// with focus on a page (not the rail, a panel, a dialog or the player). On screen it is off the
/// queue, so newer news can't push it out. Focus stays in it: Close, Back, Up or Down close it and
/// focus goes back where it was, as it does when 10 s pass untouched; Open opens the title where it
/// is in a library, else the requests. An OK in its first 0.6 s was meant for the page: it does nothing.
/// </summary>
sealed class RequestNotices : IDisposable
{
    /// <summary>A notice waits for the remote to rest this long.</summary>
    public const long IdleMs = 1_500;
    /// <summary>Untouched this long, a notice goes by itself.</summary>
    public const uint ShownMs = 10_000;
    /// <summary>OK this soon after a notice took focus was meant for what had it before.</summary>
    public const long OkGuardMs = 600;
    const uint PollMs = 250;

    readonly IRequestsSource source;
    readonly TvShell page;
    readonly Shell layers;
    readonly KeyRepeatGate gate;
    readonly Images images;
    readonly Func<bool> visible;
    readonly Action<PageKey> open;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly Timer poll = new Timer(PollMs);
    NoticeSheet? shown;
    bool disposed;

    public RequestNotices(IRequestsSource source, TvShell page, Shell layers, KeyRepeatGate gate, Images images, Func<bool> visible, Action<PageKey> open)
    {
        this.source = source;
        this.page = page;
        this.layers = layers;
        this.gate = gate;
        this.images = images;
        this.visible = visible;
        this.open = open;
        poll.Tick += (_, _) => Try();
        watches.Add(source.Notices.Watch(queue =>
        {
            if (queue.Count > 0 && shown == null && !poll.IsRunning()) poll.Start();
        }));
        // Back in front: as a press, so news waits for the remote to rest from it.
        layers.Shown += Back;
    }

    public void Dispose()
    {
        disposed = true;
        foreach (var w in watches) w.Dispose();
        layers.Shown -= Back;
        poll.Stop();
        shown?.Close(false);
    }

    void Back(bool front)
    {
        if (front) gate.Touched();
    }

    /// <summary>Whether a notice can show now; true keeps the poll going.</summary>
    bool Try()
    {
        if (shown != null) return false;
        var queue = source.Notices.Value;
        if (queue.Count == 0) return false;
        var current = page.Tree.Current;
        // Not on the requests, which show it.
        var free = visible() && !TextInput.Typing && layers.Top == page && page.Top?.Key is not PageKey.Requests && current != null && current.Parent != page.Rail.Group && gate.QuietFor() >= IdleMs;
        if (!free) return true;
        var notice = queue[0];
        source.Dismiss(notice);
        shown = new NoticeSheet(layers, notice, source.Origin, images, opens =>
        {
            shown = null;
            // The next waits for the remote to rest from the key that closed this one.
            gate.Touched();
            if (opens) open(notice is RequestNotice.Ready { ItemId: { } id } ? new PageKey.Item(id) : new PageKey.Requests());
            if (!disposed && source.Notices.Value.Count > 0 && !poll.IsRunning()) poll.Start();
        });
        shown.Show();
        return false;
    }

    /// <summary>
    /// The card: 360 wide, Surface2 at 96 % with a Surface4 hairline along its top, radius 8, 51 dp
    /// above the screen's foot at the right safe edge; the poster at 40 by 60, or a green tick without
    /// one, then the news in three lines at most, and Open with Close at its foot.
    /// </summary>
    sealed class NoticeSheet : Sheet
    {
        const float Width = 360, Bottom = 51, Pad = 16, PosterWidth = 40, PosterHeight = 60;

        readonly Shell layers;
        readonly Action<bool> closed;
        readonly Timer untouched = new Timer(ShownMs);
        readonly long shownAt = Environment.TickCount;
        bool closing;

        public NoticeSheet(Shell layers, RequestNotice notice, string origin, Images images, Action<bool> closed) : base("notice", Axis.Row)
        {
            this.layers = layers;
            this.closed = closed;
            var poster = (notice as RequestNotice.Ready)?.PosterUrl;
            var textX = Pad;
            var card = new View { CornerRadius = Ui.Dp(PleyShape.DialogRadius), BackgroundColor = RequestKit.Alpha(PleyColors.Surface2, 0.96f), Opacity = 0 };
            card.Add(new View { Size = new Size(Ui.Dp(Width), Ui.Dp(1)), BackgroundColor = PleyColors.Surface4 });
            var top = Pad;
            float headHeight;
            var words = PanelColumn.Lines(RequestText.Notice(notice), PleyType.Label, PleyColors.Fg, Width - 2 * Pad - (poster != null ? PosterWidth + 14 : 30), 3);
            if (poster != null)
            {
                var art = new ImageView
                {
                    Position = new Position(Ui.Dp(Pad), Ui.Dp(top)),
                    Size = new Size(Ui.Dp(PosterWidth), Ui.Dp(PosterHeight)),
                    CornerRadius = Ui.Dp(PleyShape.TileRadius),
                    BackgroundColor = PleyColors.Surface3,
                };
                card.Add(art);
                images.Load(new[] { Artwork.Absolute(poster, origin) }, PosterWidth, file =>
                {
                    if (file != null && !closing) art.ResourceUrl = file;
                });
                textX += PosterWidth + 14;
                headHeight = Math.Max(PosterHeight, words.SizeHeight / Ui.Scale);
            }
            else
            {
                // The poster says which title it is; without one, a tick says it's good news.
                var tick = Icons.Icon("circle-check", 20, PleyColors.Success);
                tick.Position = new Position(Ui.Dp(Pad), Ui.Dp(top));
                card.Add(tick);
                textX += 30;
                headHeight = Math.Max(20, words.SizeHeight / Ui.Scale);
            }
            words.Position = new Position(Ui.Dp(textX), Ui.Dp(top + (headHeight - words.SizeHeight / Ui.Scale) / 2));
            card.Add(words);
            var buttons = new ButtonRow();
            buttons.Add(new ButtonView(RequestText.Open, ButtonKind.Text, () => Press(true)));
            buttons.Add(new ButtonView(RequestText.Close, ButtonKind.Text, () => Press(false)));
            var height = top + headHeight + 14 + PleyTv.ButtonHeight + Pad;
            buttons.Position = new Position(Ui.Dp(Width - Pad) - Ui.Dp(buttons.WidthDp), Ui.Dp(top + headHeight + 14));
            card.Add(buttons);
            card.Size = new Size(Ui.Dp(Width), Ui.Dp(height));
            card.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - PleyTv.SafeHorizontal - Width), Ui.Dp(PleyTv.CanvasHeight - Bottom - height));
            View.Add(card);
            Root.Add(buttons.Group);
            // Up or Down close it; every key keeps it a while longer.
            buttons.Group.Exit = d =>
            {
                Touch();
                if (d is Direction.Up or Direction.Down) Close(false);
                return true;
            };
            buttons.Group.ChildFocused += _ => Touch();
            untouched.Tick += (_, _) =>
            {
                Close(false);
                return false;
            };
            Gone += () =>
            {
                untouched.Stop();
                Close(false);
            };
            var fade = new Animation(PleyMotion.RevealMs);
            fade.AnimateTo(card, "Opacity", 1f);
            fade.PlayHeld();
        }

        void Touch()
        {
            untouched.Stop();
            untouched.Start();
        }

        public void Show()
        {
            layers.Push(this);
            Tree.Enter(Root);
            untouched.Start();
        }

        /// <summary>Open or Close pressed: an OK this soon after it took focus was meant for the page.</summary>
        void Press(bool opens)
        {
            if (Environment.TickCount - shownAt >= OkGuardMs) Close(opens);
        }

        /// <summary>Closes it, once; told whether it opens.</summary>
        public void Close(bool opens)
        {
            if (closing) return;
            closing = true;
            untouched.Stop();
            layers.Pop(this);
            closed(opens);
        }
    }
}
