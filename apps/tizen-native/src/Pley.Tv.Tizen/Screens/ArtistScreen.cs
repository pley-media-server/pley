using System;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// An artist on TV (design spec §4.10; the Android app's TvArtistScreen): their backdrop; the round
/// picture (120 dp) at the gutter from y = 40, and 28 dp beside it "Artist", the name, "4 albums ·
/// 40 tracks" with their genres and two lines of their summary; Play and Shuffle (every track of
/// theirs, which opens Now Playing), and Now playing while music of theirs plays; then their
/// albums, newest first, as a row of square tiles captioned with the year. Focus starts on Play;
/// Down goes to the albums. An album opens its page; holding OK or Menu on it opens its actions.
/// </summary>
sealed class ArtistScreen : ShellPage
{
    const float Top = 40, Picture = 120, PictureGap = 28, TextWidth = 520;
    const float TextLeft = PleyTv.Gutter + Picture + PictureGap;

    readonly ArtistModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly IDisposable watch;
    readonly BackdropView backdrop;
    readonly View body = new View();
    readonly View header = new View();
    readonly View under = new View();
    readonly View albumsView = new View();
    MusicButtons? buttons;
    TextLabel? problemLabel;
    RowView? row;
    string shownKind = "";
    string shownHeader = "";
    string shownAlbums = "";
    string? shownBackdrop;
    string? problem;
    float albumsTop;

    /// <param name="go">Opens another page: an album's, Now Playing.</param>
    public ArtistScreen(TvShell shell, ArtistModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new ArtistPage(model.Id), "artist")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        backdrop = new BackdropView(images);
        View.Add(backdrop);
        View.Add(new ScrimView(ScrimView.Item));
        View.Add(body);
        model.Told += OnTold;
        watch = model.State.Watch(Show);
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Wait();
    }

    public override void Gone()
    {
        watch.Dispose();
        model.Told -= OnTold;
        model.Dispose();
    }

    void OnTold(MusicEvent e)
    {
        switch (e)
        {
            case MusicEvent.Played:
                problem = null;
                ShowProblem();
                if (Shell.Top == this) go(new PageKey.NowPlaying());
                break;
            case MusicEvent.NothingToPlay:
                problem = MusicText.NothingToPlay;
                ShowProblem();
                break;
            case MusicEvent.Failed failed:
                problem = Errors.Message(failed.Error);
                ShowProblem();
                break;
        }
    }

    void Show(MusicPageState<ArtistView> state)
    {
        switch (state)
        {
            case MusicPageState<ArtistView>.Loading:
                if (shownKind == "loading") return;
                shownKind = "loading";
                Clear();
                Skeleton();
                return;
            case MusicPageState<ArtistView>.NotFound:
                shownKind = "not-found";
                Clear();
                Message(MusicText.ArtistNotFound, MusicText.NotFoundBody, MusicText.Back, () =>
                {
                    if (Shell.Top == this) Shell.Back?.Invoke();
                });
                return;
            case MusicPageState<ArtistView>.Failed failed:
                shownKind = "failed";
                Clear();
                Message(MusicKit.FailedTitle(failed.Error, MusicText.ArtistFailed), ItemText.FailedBody(failed.Error), MusicText.Retry, model.Retry);
                return;
            case MusicPageState<ArtistView>.Ready ready:
                var first = shownKind != "ready";
                if (first) Clear();
                shownKind = "ready";
                ShowReady(ready, first);
                return;
        }
    }

    void Clear()
    {
        Group.Clear();
        foreach (var view in new[] { header, under, albumsView }) view.Unparent();
        MusicKit.Empty(header);
        MusicKit.Empty(under);
        MusicKit.Empty(albumsView);
        MusicKit.Empty(body);
        buttons = null;
        problemLabel = null;
        row = null;
        shownHeader = "";
        shownAlbums = "";
        shownBackdrop = null;
        backdrop.Show(null);
    }

    /// <summary>Still blocks in the header's shapes (the Android app's ArtistSkeleton).</summary>
    void Skeleton()
    {
        body.Add(MusicKit.Block(PleyTv.Gutter, Top, Picture, Picture, Picture / 2));
        var top = Top + (Picture - (36 + 10 + 18)) / 2;
        body.Add(MusicKit.Block(TextLeft, top, 280, 36));
        body.Add(MusicKit.Block(TextLeft, top + 36 + 10, 200, 18));
    }

    void Message(string title, string sentence, string action, Action onAction)
    {
        var buttonRow = MusicKit.Message(body, title, sentence, action, onAction);
        buttonRow.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        Group.Add(buttonRow.Group);
        if (Shell.Top == this) Tree.Enter(buttonRow.Group);
    }

    void ShowReady(MusicPageState<ArtistView>.Ready ready, bool first)
    {
        Waited();
        var artist = ready.Page;
        var back = artist.BackdropUrl != null ? Artwork.Absolute(artist.BackdropUrl, artist.Origin) : null;
        if (back != shownBackdrop)
        {
            shownBackdrop = back;
            backdrop.Show(back != null ? new[] { back } : null);
        }
        if (first)
        {
            body.Add(header);
            body.Add(under);
            body.Add(albumsView);
            buttons = new MusicButtons(() => model.Play(shuffle: false), () => model.Play(shuffle: true), () => go(new PageKey.NowPlaying()), Tree.Focus, onArt: true);
            buttons.Group.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
            under.Add(buttons);
            Group.Add(buttons.Group);
        }
        Header(artist);
        buttons!.Set(!ready.Busy, !ready.Busy, ready.Current != null);
        ShowProblem();
        Albums(artist);
        if (first && Shell.Top == this) Tree.Focus(buttons.Play);
    }

    /// <summary>
    /// The round picture, and beside it, centred on it, "Artist" in Caption Muted, the name in
    /// PageTitle (2 lines at most), the counts and genres in Label Muted and the summary in Body (2
    /// lines); then, 18 dp under the taller of the two, the buttons, and the albums under them.
    /// </summary>
    void Header(ArtistView artist)
    {
        var meta = string.Join("  ·  ", new[] { artist.Counts, string.Join(" · ", artist.Genres.Take(3)) }.Where(s => s.Length > 0));
        var picture = artist.PosterUrl != null ? Artwork.Absolute(artist.PosterUrl, artist.Origin) : null;
        var key = string.Join("\n", artist.Title, picture ?? "", meta, artist.Summary ?? "");
        if (key == shownHeader) return;
        shownHeader = key;
        MusicKit.Empty(header);

        var circle = new ArtWell(Picture, Picture / 2, images, picture);
        header.Add(circle);
        var lines = new View();
        float h = 0;
        var kind = Text.Label(MusicText.Artist, PleyType.Caption, PleyColors.Muted, width: TextWidth);
        lines.Add(kind);
        h += PleyType.Caption.Line;
        var name = PanelColumn.Lines(artist.Title, PleyType.PageTitle, PleyColors.Fg, TextWidth, 2);
        name.PositionY = Ui.Dp(h);
        lines.Add(name);
        h += name.SizeHeight / Ui.Scale;
        if (meta.Length > 0)
        {
            var line = Text.Label(meta, PleyType.Label, PleyColors.Muted, width: TextWidth);
            line.PositionY = Ui.Dp(h + 6);
            lines.Add(line);
            h += 6 + PleyType.Label.Line;
        }
        if (artist.Summary is { } summary)
        {
            var words = PanelColumn.Lines(summary, PleyType.Body, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.85f), TextWidth, 2);
            words.PositionY = Ui.Dp(h + 8);
            lines.Add(words);
            h += 8 + words.SizeHeight / Ui.Scale;
        }
        var tall = Math.Max(Picture, h);
        circle.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(Top + (tall - Picture) / 2));
        lines.Position = new Position(Ui.Dp(TextLeft), Ui.Dp(Top + (tall - h) / 2));
        header.Add(lines);
        var y = Top + tall + 18;
        under.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        albumsTop = y + PleyTv.ButtonHeight + MusicKit.ButtonLabelRoom + 4;
        albumsView.PositionY = Ui.Dp(albumsTop);
    }

    /// <summary>The problem beside the buttons, in Caption DangerText: nothing to play, or why playing failed.</summary>
    void ShowProblem()
    {
        if (problemLabel != null)
        {
            problemLabel.Unparent();
            problemLabel.Dispose();
            problemLabel = null;
        }
        if (problem == null || buttons == null) return;
        var x = buttons.EndDp + 8;
        problemLabel = Text.Label(problem, PleyType.Caption, PleyColors.DangerText, width: PleyTv.CanvasWidth - PleyTv.SafeHorizontal - PleyTv.Gutter - x);
        problemLabel.Position = new Position(Ui.Dp(x), Ui.Dp(14));
        under.Add(problemLabel);
    }

    /// <summary>"Loading…" or "No albums found." while there are none; else the albums' row, made again only when they changed.</summary>
    void Albums(ArtistView artist)
    {
        var key = artist.Albums.Count == 0
            ? (artist.AlbumsLoading ? "loading" : "empty")
            : string.Join(",", artist.Albums.Select(a => $"{a.Id}/{a.Title}/{a.Year}/{a.PosterUrl}"));
        if (key == shownAlbums) return;
        shownAlbums = key;
        var focusedName = row != null && Tree.Current != null && Tree.Current.Parent == row.Group ? Tree.Current.Name : null;
        if (row != null) Group.Remove(row.Group);
        row = null;
        MusicKit.Empty(albumsView);
        if (artist.Albums.Count == 0)
        {
            var note = Text.Label(artist.AlbumsLoading ? MusicText.Loading : MusicText.NoAlbums, PleyType.Body, PleyColors.Muted, width: TextWidth);
            note.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(12));
            albumsView.Add(note);
            if (focusedName != null && buttons != null) Tree.Focus(buttons.Play);
            return;
        }
        var tiles = artist.Albums
            .Select(a => LibraryRules.Tile(a, TileLook.Square, artist.Origin) with { Subtitle = a.Year?.ToString(System.Globalization.CultureInfo.InvariantCulture) ?? "" })
            .ToList();
        row = new RowView(MusicText.Albums, tiles, TileKind.Square, gate, images,
            tile => go(new AlbumPage(tile.Id)),
            tile => Shell.Actions?.Invoke(tile.Id, false),
            Shell.FocusRail, focusItem: Tree.Focus);
        albumsView.Add(row);
        Group.Add(row.Group);
        row.ShowPictures();
        if (focusedName != null)
        {
            var again = row.Group.Children.OfType<FocusItem>().FirstOrDefault(i => i.Name == focusedName);
            if (again != null) Tree.Focus(again);
            else Tree.Enter(row.Group);
        }
    }
}
