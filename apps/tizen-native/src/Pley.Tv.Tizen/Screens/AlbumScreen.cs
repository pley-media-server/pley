using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// An album on TV (design spec §4.10; the Android app's TvAlbumScreen): the art (220 dp) and what it
/// is in a 280 dp column at the gutter from y = 40 (the title, the artist as a link to their page,
/// the year, the tracks and their length, the profile's rating as stars), Play, Shuffle and Now
/// playing under it; the tracks on the right with disc headers, the one playing marked. OK on a
/// track plays the album from it, or pauses the track playing. Playing opens Now Playing; Back
/// comes here. Over the art at 14 %. Focus starts on Play, or on the track it was opened from.
/// </summary>
sealed class AlbumScreen : ShellPage
{
    const float Top = 40, Art = 220, Column = 280, ColumnGap = 24, ListBottom = 120;
    const float ListLeft = PleyTv.Gutter + Column + ColumnGap, ListWidth = PleyTv.CanvasWidth - PleyTv.SafeHorizontal - ListLeft;

    readonly AlbumModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly IDisposable watch;
    readonly FaintArtView faint;
    readonly View body = new View();
    readonly View header = new View();
    readonly View under = new View();
    readonly FocusGroup page = new FocusGroup(Axis.Row, "album");
    readonly FocusGroup column = new FocusGroup(Axis.Column, "album-column");
    MusicButtons? buttons;
    ButtonView? artistLink;
    TrackListView? list;
    View? listNote;
    TextLabel? problemLabel;
    string shownKind = "";
    string shownHeader = "";
    string? shownNote;
    string? problem;
    /// <summary>The page has put focus where it starts (Play, or the track it was opened from).</summary>
    bool started;
    /// <summary>The tracks have had focus: coming back to them lands where focus was, not on the entry.</summary>
    bool listVisited;

    /// <param name="go">Opens another page: the artist's, Now Playing.</param>
    public AlbumScreen(TvShell shell, AlbumModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new AlbumPage(model.Id, model.HighlightId), "album")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        faint = new FaintArtView(images, 0.14f);
        View.Add(faint);
        View.Add(body);
        column.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        // Right from the column with no tracks to go to stays put.
        page.Exit = d => d == Direction.Right;
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

    void Show(MusicPageState<AlbumView> state)
    {
        switch (state)
        {
            case MusicPageState<AlbumView>.Loading:
                if (shownKind == "loading") return;
                shownKind = "loading";
                Clear();
                Skeleton();
                return;
            case MusicPageState<AlbumView>.NotFound:
                shownKind = "not-found";
                Clear();
                Message(MusicText.AlbumNotFound, MusicText.NotFoundBody, MusicText.Back, () =>
                {
                    if (Shell.Top == this) Shell.Back?.Invoke();
                });
                return;
            case MusicPageState<AlbumView>.Failed failed:
                shownKind = "failed";
                Clear();
                Message(MusicKit.FailedTitle(failed.Error, MusicText.AlbumFailed), ItemText.FailedBody(failed.Error), MusicText.Retry, model.Retry);
                return;
            case MusicPageState<AlbumView>.Ready ready:
                if (shownKind != "ready") Clear();
                shownKind = "ready";
                ShowReady(ready);
                return;
        }
    }

    void Clear()
    {
        Group.Clear();
        page.Clear();
        column.Clear();
        // The header and the buttons' box stay for the next page; what they hold goes.
        header.Unparent();
        under.Unparent();
        MusicKit.Empty(header);
        MusicKit.Empty(under);
        MusicKit.Empty(body);
        buttons = null;
        artistLink = null;
        list = null;
        listNote = null;
        problemLabel = null;
        shownHeader = "";
        shownNote = null;
        started = false;
        listVisited = false;
        faint.Show(null);
    }

    /// <summary>Still blocks in the page's shapes (the Android app's AlbumSkeleton).</summary>
    void Skeleton()
    {
        var x = PleyTv.Gutter;
        body.Add(MusicKit.Block(x, Top, Art, Art, PleyShape.TileRadius));
        body.Add(MusicKit.Block(x, Top + Art + 14, 180, 22));
        body.Add(MusicKit.Block(x, Top + Art + 14 + 22 + 8, 120, 16));
        for (var i = 0; i < 8; i++) body.Add(MusicKit.Block(x + Art + 40, Top + i * (34 + 10), 480, 34, PleyShape.FieldRadius));
    }

    void Message(string title, string sentence, string action, Action onAction)
    {
        var row = MusicKit.Message(body, title, sentence, action, onAction);
        row.Group.Exit = d => d == Direction.Left && Scrolling.Left(gate, Shell.FocusRail);
        Group.Add(row.Group);
        if (Shell.Top == this) Tree.Enter(row.Group);
    }

    void ShowReady(MusicPageState<AlbumView>.Ready ready)
    {
        var album = ready.Page;
        var art = album.PosterUrl != null ? Artwork.Absolute(album.PosterUrl, album.Origin) : null;
        faint.Show(art);
        if (list == null)
        {
            body.Add(header);
            body.Add(under);
            Group.Add(page);
            page.Add(column);
            buttons = new MusicButtons(() => model.Play(0), model.Shuffle, () => go(new PageKey.NowPlaying()), Tree.Focus, onArt: false);
            under.Add(buttons);
            list = new TrackListView(ListWidth, PleyTv.CanvasHeight - Top, gate, Pick, Tree.Focus, ListBottom, "album-tracks");
            list.Position = new Position(Ui.Dp(ListLeft), Ui.Dp(Top));
            list.Group.Exit = d => d == Direction.Right;
            list.Group.ChildFocused += _ => listVisited = true;
            body.Add(list);
            page.Add(list.Group);
            ShowProblem();
        }
        Header(album, art);
        buttons!.Set(album.TracksLoading || album.Tracks.Count > 0, album.Tracks.Count > 0, ready.Current != null);
        Tracks(album, ready);
        Start(album);
    }

    /// <summary>
    /// The column: the art, 12 dp, the title (Title, 2 lines at most), the artist (a link to their
    /// page when known), the year and "10 tracks · 42m" in Caption Muted, the stars; then 16 dp and
    /// the buttons. Made again only when what it says changed.
    /// </summary>
    void Header(AlbumView album, string? art)
    {
        var key = string.Join("\n", album.Title, art ?? "", album.Artist ?? "", album.ArtistId?.ToString() ?? "", album.Line, album.UserRating?.ToString() ?? "");
        if (key == shownHeader) return;
        shownHeader = key;
        var linkFocused = artistLink?.Item.Focused == true;
        var old = header.Children.ToList();
        foreach (var child in old) header.Remove(child);
        if (artistLink != null) column.Remove(artistLink.Item);
        artistLink = null;

        var x = PleyTv.Gutter;
        float y = Top;
        var well = new ArtWell(Art, PleyShape.TileRadius, images, art);
        well.Position = new Position(Ui.Dp(x), Ui.Dp(y));
        header.Add(well);
        y += Art + 12;
        var title = PanelColumn.Lines(album.Title, PleyType.Title, PleyColors.Fg, Column, 2);
        title.Position = new Position(Ui.Dp(x), Ui.Dp(y));
        header.Add(title);
        y += title.SizeHeight / Ui.Scale;
        if (album.Artist is { } artist)
        {
            if (album.ArtistId is { } artistId)
            {
                // The link's own padding sits outside the column's edge, so its text lines up with the title's.
                artistLink = new ButtonView(artist, ButtonKind.Text, () => go(new ArtistPage(artistId)), PleyColors.AccentBright, maxWidth: Column + 16) { Rest = Color.Transparent };
                artistLink.Position = new Position(Ui.Dp(x - 16), Ui.Dp(y + 2));
                header.Add(artistLink);
                column.Insert(0, artistLink.Item);
                y += 2 + PleyTv.ButtonHeight;
            }
            else
            {
                var name = Text.Label(artist, PleyType.Label, PleyColors.Fg, width: Column);
                name.Position = new Position(Ui.Dp(x), Ui.Dp(y));
                header.Add(name);
                y += PleyType.Label.Line;
            }
        }
        if (album.Line.Length > 0)
        {
            var meta = Text.Label(album.Line, PleyType.Caption, PleyColors.Muted, width: Column);
            meta.Position = new Position(Ui.Dp(x), Ui.Dp(y + 4));
            header.Add(meta);
            y += 4 + PleyType.Caption.Line;
        }
        if (album.UserRating is { } rating)
        {
            var stars = MusicKit.Stars(rating);
            stars.Position = new Position(Ui.Dp(x), Ui.Dp(y + 6));
            header.Add(stars);
            y += 6 + 16;
        }
        y += 16;
        under.Position = new Position(Ui.Dp(x), Ui.Dp(y));
        if (buttons != null && !column.Children.Contains(buttons.Group)) column.Add(buttons.Group);
        if (linkFocused && artistLink != null) Tree.Focus(artistLink.Item);
        else if (linkFocused && buttons != null) Tree.Focus(buttons.Play);
        foreach (var child in old) child.Dispose();
    }

    /// <summary>The problem under the buttons, in Caption DangerText: nothing to play, or why playing failed.</summary>
    void ShowProblem()
    {
        if (problemLabel != null)
        {
            problemLabel.Unparent();
            problemLabel.Dispose();
            problemLabel = null;
        }
        if (problem == null || buttons == null) return;
        problemLabel = PanelColumn.Lines(problem, PleyType.Caption, PleyColors.DangerText, Column, 2);
        problemLabel.PositionY = Ui.Dp(PleyTv.ButtonHeight + MusicKit.ButtonLabelRoom + 8);
        under.Add(problemLabel);
    }

    /// <summary>
    /// The tracks with their disc headers; skeleton lines while they load, "No tracks found" when
    /// there are none. Until the list has had focus, coming into it lands on the track the album
    /// was opened from, else the one playing, else the first.
    /// </summary>
    void Tracks(AlbumView album, MusicPageState<AlbumView>.Ready ready)
    {
        var highlight = Highlight(album);
        var lines = album.Rows.Select(row => row switch
        {
            AlbumRow.Disc d => new TrackLine(d.Key, d.Label, Header: true),
            AlbumRow.Track t => new TrackLine(t.Key, t.Title, Number: t.Number, Subtitle: t.Artist, Duration: t.Duration,
                Current: ready.Current == t.Id, Playing: ready.Current == t.Id && ready.Playing, Highlighted: highlight == t.Id),
            _ => throw new InvalidOperationException(),
        }).ToList();
        list!.Set(lines);
        var note = album.TracksLoading ? "loading" : album.Tracks.Count == 0 ? "empty" : null;
        if (note != shownNote)
        {
            shownNote = note;
            if (listNote != null)
            {
                listNote.Unparent();
                listNote.Dispose();
                listNote = null;
            }
            if (note != null)
            {
                listNote = new View { Position = new Position(Ui.Dp(ListLeft), Ui.Dp(Top)) };
                if (note == "loading")
                    for (var i = 0; i < 8; i++) listNote.Add(MusicKit.Block(0, 4 + i * (36 + 8 + 2), 480, 36, PleyShape.FieldRadius));
                else listNote.Add(Text.Label(MusicText.NoTracks, PleyType.Body, PleyColors.Muted, width: ListWidth));
                body.Add(listNote);
            }
        }
        if (listVisited) return;
        var entry = highlight ?? (ready.Current is { } c && album.Tracks.Any(t => t.Id == c) ? c : album.Tracks.FirstOrDefault()?.Id);
        if (entry is { } id && KeyOf(album, id) is { } key) list.Remember(key);
    }

    /// <summary>The track the album was opened from, when it is one of the album's.</summary>
    long? Highlight(AlbumView album) =>
        model.HighlightId is { } id && album.Tracks.Any(t => t.Id == id) ? id : null;

    static string? KeyOf(AlbumView album, long trackId) => album.Rows.OfType<AlbumRow.Track>().FirstOrDefault(t => t.Id == trackId)?.Key;

    /// <summary>
    /// Where focus starts, once: the track the album was opened from, scrolled to with three lines
    /// above it (as soon as the tracks are there), else Play. Set as what the page's groups remember,
    /// so it holds for the page opening as for an answer coming in.
    /// </summary>
    void Start(AlbumView album)
    {
        if (started || buttons == null || list == null) return;
        Waited();
        if (model.HighlightId != null && album.TracksLoading)
        {
            // Opened from a track: it takes focus once the tracks come; Play holds it meanwhile.
            AtPlay();
            if (Shell.Top == this && !InPage(Tree.Current)) Tree.Enter(Group);
            return;
        }
        started = true;
        if (Highlight(album) is { } id && KeyOf(album, id) is { } key)
        {
            page.RememberChild(1);
            list.Remember(key);
            list.ShowLine(key, linesAbove: 3);
        }
        else AtPlay();
        if (Shell.Top == this) Tree.Enter(Group);
    }

    void AtPlay()
    {
        page.RememberChild(0);
        column.RememberChild(column.Children.ToList().IndexOf(buttons!.Group));
        buttons.Group.RememberChild(0);
    }

    bool InPage(FocusNode? node)
    {
        for (var n = node?.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }

    void Pick(string key)
    {
        if (model.State.Value is not MusicPageState<AlbumView>.Ready ready) return;
        var row = ready.Page.Rows.OfType<AlbumRow.Track>().FirstOrDefault(t => t.Key == key);
        if (row != null) model.PlayOrToggle(row.Position);
    }
}
