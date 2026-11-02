using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Pley.Tv.Core;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Local;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A DVD's disc image on the drive without menus to play it from (<see cref="DvdMenuScreen"/>
/// plays those that have them): its titles, the longest (the main feature) first, each with its
/// length, chapters and audio languages, and where it resumes; OK plays a title in Pley's player. Titles under half a minute (logos, warnings) and a title playing the same cells
/// as one listed aren't shown.
/// </summary>
sealed class DiscScreen : ShellPage
{
    const float ListTop = PleyTv.SafeVertical + 34 + 8 + 18 + 16;
    const float ListWidth = 720, ListBottom = 120;
    /// <summary>Shorter titles are a disc's logos and warnings.</summary>
    const long ShortestMs = 30_000;

    readonly string image;
    readonly IKeyValue storage;
    readonly Action<string, DvdTitle, long> play;
    readonly TrackListView list;
    readonly TextLabel line;
    readonly View message = new View();
    IReadOnlyList<DvdTitle> titles = Array.Empty<DvdTitle>();
    bool gone;

    /// <param name="play">Plays a title of the image, from a position.</param>
    public DiscScreen(TvShell shell, string image, IKeyValue storage, KeyRepeatGate gate, Action<string, DvdTitle, long> play)
        : base(shell, new PageKey.Disc(image), "disc")
    {
        this.image = image;
        this.storage = storage;
        this.play = play;
        var title = Text.Label(LocalFolder.Title(image), PleyType.PageTitle, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(title);
        line = Text.Label("DVD", PleyType.Caption, PleyColors.Muted, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        line.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical + 34 + 8));
        View.Add(line);
        list = new TrackListView(ListWidth, PleyTv.CanvasHeight - ListTop, gate, Pick, Tree.Focus, ListBottom, "disc-titles");
        list.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ListTop));
        list.Group.Exit = d => d switch
        {
            Direction.Left => Scrolling.Left(gate, Shell.FocusRail),
            Direction.Right => true,
            _ => false,
        };
        View.Add(list);
        View.Add(message);
        Group.Add(list.Group);
        Load();
    }

    public override void Enter()
    {
        // Back from the player: a title's line says where it resumes now.
        Lines();
        if (!Tree.Enter(list.Group)) Wait();
    }

    public override void Gone() => gone = true;

    /// <summary>
    /// The image's titles, and its navigation (null when it has none, or it can't be read: its
    /// titles play without its menus). Its IFOs are read off the drive, which may sleep: off the UI thread.
    /// </summary>
    public static async Task<(IReadOnlyList<DvdTitle> Titles, DvdDisc? Disc)> Read(string image)
    {
        try
        {
            return await Task.Run(() =>
            {
                using var f = new FileStream(image, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 4096);
                byte[] Read(long offset, int count)
                {
                    if (offset >= f.Length) return new byte[0];
                    f.Seek(offset, SeekOrigin.Begin);
                    var bytes = new byte[(int)Math.Min(count, f.Length - offset)];
                    var read = 0;
                    while (read < bytes.Length)
                    {
                        var n = f.Read(bytes, read, bytes.Length - read);
                        if (n == 0) break;
                        read += n;
                    }
                    if (read < bytes.Length) Array.Resize(ref bytes, read);
                    return bytes;
                }
                DvdDisc? disc = null;
                try
                {
                    disc = DvdDisc.Read(Read);
                }
                catch (Exception e)
                {
                    Profile.Mark($"disc: its navigation failed: {e.GetType().Name}");
                }
                return ((IReadOnlyList<DvdTitle>)DvdVideo.Titles(Read), disc);
            });
        }
        catch (Exception)
        {
            return (Array.Empty<DvdTitle>(), null);
        }
    }

    /// <summary>Whether the disc has menus to play it from, and titles for them to play.</summary>
    public static bool HasMenus(DvdDisc? disc, IReadOnlyList<DvdTitle> titles) =>
        disc != null && titles.Count > 0 && (disc.VmgMenus.Count > 0 || disc.Sets.Values.Any(s => s.Menus.Count > 0));

    async void Load()
    {
        var (found, _) = await Read(image);
        if (gone) return;
        // Logos and warnings out; a title playing the same cells as a longer one listed out too.
        var seen = new HashSet<string>();
        titles = found.Where(t => t.DurationMs >= ShortestMs && seen.Add(string.Join(",", t.Spans.Select(s => s.Offset)))).ToList();
        if (Marks.On) Marks.Note($"disc: {found.Count} titles, {titles.Count} shown");
        Text.Show(line, titles.Count == 0 ? "" : $"DVD · {titles.Count} {(titles.Count == 1 ? "title" : "titles")}");
        if (titles.Count == 0)
        {
            var width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
            message.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ListTop + 40));
            message.Add(Text.Label("This disc can't be played here", PleyType.Title, PleyColors.Fg, width: width));
            var words = Text.Label("It isn't a DVD-Video image this TV can read: a Blu-ray, an encrypted DVD or another kind of disc.", PleyType.Body, PleyColors.Muted, maxLines: 3, width: width);
            words.VerticalAlignment = VerticalAlignment.Top;
            words.PositionY = Ui.Dp(PleyType.Title.Line + 10);
            message.Add(words);
        }
        Lines();
        if (Shell.Top == this && list.Group.Children.Count > 0) Tree.Enter(list.Group);
    }

    /// <summary>A title's line: its number ("Main feature" for the longest), length, chapters and audio, and where it resumes.</summary>
    void Lines()
    {
        list.Set(titles.Select((t, i) =>
        {
            var key = LocalPlayerSource.DiscKey(image, t.Number);
            var resume = LocalPlayerSource.ResumeOf(storage, key);
            var languages = t.Streams.Where(s => s.Kind == "audio").Select(s => ItemFormat.LanguageName(s.Language) ?? "").Where(l => l.Length > 0).Distinct().ToList();
            var facts = new List<string>();
            if (t.Chapters.Count > 1) facts.Add($"{t.Chapters.Count} chapters");
            if (languages.Count > 0) facts.Add(string.Join(", ", languages));
            if (resume > 0) facts.Add($"Resumes at {PlayerText.Time(resume)}");
            return new TrackLine(key, i == 0 ? $"Main feature · Title {t.Number}" : $"Title {t.Number}",
                Subtitle: string.Join(" · ", facts),
                Duration: PlayerText.Time(t.DurationMs),
                Icon: "film",
                Progress: resume > 0 && t.DurationMs > 0 ? Math.Max(0.02f, (float)resume / t.DurationMs) : 0);
        }).ToList());
    }

    void Pick(string key)
    {
        var title = titles.FirstOrDefault(t => LocalPlayerSource.DiscKey(image, t.Number) == key);
        if (title != null) play(image, title, LocalPlayerSource.ResumeOf(storage, key));
    }
}
