using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The pieces the TV's request screens share (the Android app's TvRequestsKit): posters with their
/// status badges, stage pills in their colours, the download bar, the number on the way in, the page
/// a problem leaves, and signing in again.
/// </summary>
static class RequestKit
{
    /// <summary>The words' clock: now, in the TV's time zone.</summary>
    public static readonly RequestText Text = new RequestText(() => DateTimeOffset.UtcNow.ToUnixTimeSeconds());

    /// <summary>A title's id in rows and grids: a movie and a show may share a TMDB id.</summary>
    public static long TileId(DiscoverTitle title) => title.TmdbId * 2 + (title.Kind == TitleKind.Show ? 1 : 0);

    /// <summary>"2024 · Movie".</summary>
    public static string TitleLine(int? year, string kind) =>
        string.Join(" · ", new[] { year?.ToString(CultureInfo.InvariantCulture), RequestText.Kind(kind) }.Where(s => s != null));

    static IReadOnlyList<string> Picture(string? url, string origin) =>
        string.IsNullOrEmpty(url) ? Array.Empty<string>() : new[] { Artwork.Absolute(url!, origin) };

    /// <summary><paramref name="title"/>'s poster: its year and kind under it, its status on it ("In library", "Requested").</summary>
    public static TileModel TitleTile(DiscoverTitle title, string origin)
    {
        var badge = RequestRules.BadgeOf(title) is { } b ? RequestText.Badge(b) : null;
        var said = string.Join(", ", new[] { title.Title, title.Year?.ToString(CultureInfo.InvariantCulture), RequestText.Kind(title.Kind), badge }.Where(s => s != null));
        return new TileModel(TileId(title), title.Title, TitleLine(title.Year, title.Kind), Picture(title.PosterUrl, origin), 0f, false, 0,
            badge == null ? Array.Empty<string>() : new[] { badge }, said);
    }

    /// <summary><paramref name="request"/> as a wide tile: its backdrop (or poster), the download's progress on it, its stage under it.</summary>
    public static TileModel RequestTile(MediaRequest request, string origin, bool manage)
    {
        var stage = Text.Pill(RequestRules.Pill(request, manage));
        var art = new[] { request.BackdropUrl, request.PosterUrl }.Where(u => !string.IsNullOrEmpty(u)).Select(u => Artwork.Absolute(u!, origin)).ToList();
        return new TileModel(request.Id, request.Title, stage, art, RequestRules.DownloadProgress(request) ?? 0f, false, 0, Array.Empty<string>(), $"{request.Title}, {stage}");
    }

    /// <summary>A list title's facts, before its page is read: its year and kind, TMDB's rating.</summary>
    public static IReadOnlyList<MetaPart> TitleMeta(DiscoverTitle title)
    {
        var parts = new List<MetaPart> { new MetaPart.Text(TitleLine(title.Year, title.Kind)) };
        if (title.Rating is > 0 and var r) parts.Add(new MetaPart.Rating(r.ToString("0.0", CultureInfo.InvariantCulture)));
        return parts;
    }

    /// <summary>
    /// A title's facts as the item page's line has them: its year (else whether it's a movie or a
    /// show), its certification boxed, its runtime or how many seasons, TMDB's rating after a star.
    /// </summary>
    public static IReadOnlyList<MetaPart> TitleMeta(DiscoverDetail detail)
    {
        var parts = new List<MetaPart> { new MetaPart.Text(detail.Year?.ToString(CultureInfo.InvariantCulture) ?? RequestText.Kind(detail.Kind)) };
        if (!string.IsNullOrWhiteSpace(detail.Certification)) parts.Add(new MetaPart.ContentRating(detail.Certification!));
        var seasons = (detail.Seasons ?? Array.Empty<DiscoverSeason>()).Count(s => s.Number > 0);
        var length = detail.Kind == TitleKind.Show
            ? seasons > 0 ? Format.Pluralize(seasons, "season") : null
            : detail.RuntimeMinutes is > 0 and var m ? Format.Runtime(m * 60_000L) : null;
        if (!string.IsNullOrEmpty(length)) parts.Add(new MetaPart.Text(length!));
        if (detail.Rating is > 0 and var r) parts.Add(new MetaPart.Rating(r.ToString("0.0", CultureInfo.InvariantCulture)));
        return parts;
    }

    // ---- stages

    /// <summary>A stage's colours: its words and dot, and the tint behind them.</summary>
    public static (Color Content, Color Container) Tone(PillTone tone) => tone switch
    {
        PillTone.Amber => (PleyColors.Amber, Alpha(PleyColors.Amber, 0.2f)),
        PillTone.Blue => (PleyColors.AccentBright, Alpha(PleyColors.Info, 0.2f)),
        PillTone.Green => (PleyColors.Success, Alpha(PleyColors.Success, 0.2f)),
        PillTone.Red => (PleyColors.DangerText, Alpha(PleyColors.DangerText, 0.2f)),
        _ => (PleyColors.Muted, Alpha(PleyColors.Muted, 0.16f)),
    };

    public static Color Alpha(Color c, float a) => new Color(c.R, c.G, c.B, a);

    /// <summary>
    /// A request's stage: its words after a dot, tinted by what it means. 26 dp tall beside a
    /// button, 22 <paramref name="small"/> under a title's facts; at most <paramref name="maxWidth"/> wide.
    /// </summary>
    public static View StagePill(StagePill pill, bool small = false, float maxWidth = 260)
    {
        var (content, container) = Tone(pill.Tone);
        var height = small ? 22f : 26f;
        var start = small ? 8f : 10f;
        var end = small ? 10f : 12f;
        var dot = small ? 6f : 8f;
        var gap = small ? 6f : 8f;
        var style = small ? PleyType.Caption : PleyType.Label;
        var words = Kit.Text.Label(Text.Pill(pill), style, content);
        var room = maxWidth - start - dot - gap - end;
        var wordsWidth = Math.Min(words.NaturalSize2D.Width / Ui.Scale + 2, room);
        words.Size = new Size(Ui.Dp(wordsWidth), Ui.Dp(height));
        words.VerticalAlignment = VerticalAlignment.Center;
        words.Position = new Position(Ui.Dp(start + dot + gap), 0);
        var view = new View
        {
            Size = new Size(Ui.Dp(start + dot + gap + wordsWidth + end), Ui.Dp(height)),
            CornerRadius = Ui.Dp(height / 2),
            BackgroundColor = container,
        };
        view.Add(new View
        {
            Size = new Size(Ui.Dp(dot), Ui.Dp(dot)),
            Position = new Position(Ui.Dp(start), Ui.Dp((height - dot) / 2)),
            CornerRadius = Ui.Dp(dot / 2),
            BackgroundColor = content,
        });
        view.Add(words);
        return view;
    }

    /// <summary>A download's progress, 4 dp tall and <paramref name="width"/> wide: blue, gray while paused, red with a problem.</summary>
    public static View DownloadBar(float progress, string state, float width)
    {
        var fill = state switch
        {
            DownloadState.Warning => PleyColors.Danger,
            DownloadState.Paused => PleyColors.Subtle,
            _ => PleyColors.Info,
        };
        var track = new View
        {
            Size = new Size(Ui.Dp(width), Ui.Dp(4)),
            CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
            BackgroundColor = Alpha(PleyColors.Fg, 0.16f),
        };
        var value = Math.Max(0, Math.Min(1, progress));
        if (value > 0) track.Add(new View { Size = new Size(Ui.Dp(width * value), Ui.Dp(4)), CornerRadius = Ui.Dp(PleyShape.BadgeRadius), BackgroundColor = fill });
        return track;
    }

    /// <summary><paramref name="request"/>'s pill with its download's bar 4 dp under it, as wide as the pill.</summary>
    public static View StageBlock(MediaRequest request, bool manage, float maxWidth = 260)
    {
        var pill = StagePill(RequestRules.Pill(request, manage), maxWidth: maxWidth);
        if (request.Download is not { } download) return pill;
        var block = new View { Size = new Size(pill.SizeWidth, pill.SizeHeight + Ui.Dp(8)) };
        block.Add(pill);
        var bar = DownloadBar(RequestRules.DownloadProgress(request) ?? 0f, download.State, pill.SizeWidth / Ui.Scale);
        bar.PositionY = pill.SizeHeight + Ui.Dp(4);
        block.Add(bar);
        return block;
    }

    // ---- problems

    /// <summary><paramref name="problem"/>'s heading: "Can't reach your server", <paramref name="failed"/> over any other failure, else its own words.</summary>
    public static string ProblemTitle(RequestProblem problem, RequestSummary? summary, string failed) => problem switch
    {
        RequestProblem.Offline o => Errors.Unreachable(o.Error) ? Errors.OfflineTitle : failed,
        RequestProblem.Other => failed,
        _ => Text.Problem(problem, summary),
    };

    /// <summary>The line under <see cref="ProblemTitle"/>: what to check, a failure's own sentence, or what can be done.</summary>
    public static string? ProblemBody(RequestProblem problem, RequestSummary? summary) => problem switch
    {
        RequestProblem.Offline o => Errors.Unreachable(o.Error) ? Errors.OfflineBody(o.Error) : Errors.Message(o.Error),
        RequestProblem.Other o => Errors.Message(o.Error),
        _ => RequestText.ProblemBody(problem, summary),
    };

    public static string ProblemIcon(RequestProblem problem) => problem switch
    {
        RequestProblem.Off => "compass",
        RequestProblem.NoAccess or RequestProblem.Restricted => "lock",
        RequestProblem.SignInAgain => "log-out",
        RequestProblem.TmdbKey => "triangle-alert",
        RequestProblem.TmdbDown or RequestProblem.Offline => "server-off",
        _ => "circle-alert",
    };

    /// <summary>A button in place of a page: its label, icon and what it does.</summary>
    public sealed record NoticeAction(string Label, string Icon, Action Press);

    /// <summary>
    /// A sentence in place of the page (design spec §8): an icon, the title in PageTitle and the body
    /// muted, at the gutter from y = <paramref name="top"/>, and one focused button when there is an
    /// action. Its button's group joins <paramref name="group"/>; without one, the caller sends focus to the rail.
    /// </summary>
    public static View Notice(string title, string? body, string icon, NoticeAction? action, FocusGroup group, float top = 120)
    {
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(top)) };
        view.Add(Icons.Icon(icon, 40, PleyColors.Muted));
        var y = 40f + 16;
        var heading = PanelColumn.Lines(title, PleyType.PageTitle, PleyColors.Fg, 560, 2);
        heading.PositionY = Ui.Dp(y);
        view.Add(heading);
        y += heading.SizeHeight / Ui.Scale;
        if (body != null)
        {
            y += 8;
            var words = PanelColumn.Lines(body, PleyType.Body, PleyColors.Muted, 464, 4);
            words.PositionY = Ui.Dp(y);
            view.Add(words);
            y += words.SizeHeight / Ui.Scale;
        }
        if (action != null)
        {
            y += 24;
            var row = new ButtonRow();
            row.Add(new ButtonView(action.Label, ButtonKind.Pill, action.Press, iconName: action.Icon));
            row.PositionY = Ui.Dp(y);
            view.Add(row);
            group.Add(row.Group);
        }
        return view;
    }

    /// <summary>
    /// The button a problem's page offers: Retry where trying again may help, Sign in for an
    /// old sign-in, the requests for an account that may not ask but has some.
    /// </summary>
    public static NoticeAction? ProblemAction(RequestProblem problem, RequestSummary? summary, Action retry, Action signIn, Action requests)
    {
        if (problem.Retry) return new NoticeAction(RequestText.Retry, "refresh-cw", retry);
        if (problem is RequestProblem.SignInAgain) return new NoticeAction(RequestText.SignIn, "log-out", signIn);
        if (problem is RequestProblem.NoAccess && summary is { RequestsEntry: true } s && (s.Own > 0 || s.Manage))
            return new NoticeAction(s.Manage ? RequestText.Manage : RequestText.Yours, "list-video", requests);
        return null;
    }

    /// <summary>
    /// Signing in again, so this TV may ask for movies and shows (or decide on them): the link screen
    /// of <paramref name="server"/> with a new code, whose approval replaces this sign-in. Cancel, or
    /// Sign in with focus.
    /// </summary>
    public static void SignInAgain(Shell shell, string server, Action<PageKey> restart)
    {
        Dialog? dialog = null;
        dialog = new Dialog(shell, RequestText.SignInTitle, RequestText.SignInBody(Pley.Tv.Core.Auth.Servers.HostPort(server)));
        dialog.Button(RequestText.Cancel, () => dialog!.Close());
        dialog.Button(RequestText.SignIn, () =>
        {
            dialog!.Close();
            restart(new PageKey.Link(server));
        }, initial: true);
        dialog.Show();
    }

    /// <summary>What a request screen tells once, as a toast: what asking did, why something failed; and where to go.</summary>
    public static void Tell(TvShell shell, RequestEvent e, RequestSummary? summary, Action<PageKey> go)
    {
        switch (e)
        {
            case RequestEvent.Asked asked:
                shell.Toast(RequestText.Asked(asked.Value, asked.Title));
                break;
            case RequestEvent.Failed failed:
                shell.Toast(Text.Problem(failed.Problem, summary));
                break;
            case RequestEvent.Open open:
                go(open.Key);
                break;
        }
    }

    /// <summary>"Details and pictures from TMDB.", at the foot of Discover and of every title page.</summary>
    public static TextLabel Credit()
    {
        var label = Kit.Text.Label(RequestText.Credit, PleyType.Caption, PleyColors.Subtle, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        label.PositionX = Ui.Dp(PleyTv.Gutter);
        return label;
    }
}
