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
/// A folder of the drive plugged into the TV: its folders, then its videos, a line each. OK on a
/// folder opens it over this one (Back comes back up), on a video plays it in Pley's player from
/// where it stopped. A video started before says where it will resume. Hidden files and what
/// Windows and macOS leave on a drive aren't shown.
/// </summary>
sealed class StorageScreen : ShellPage
{
    const float ListTop = PleyTv.SafeVertical + 34 + 8 + 18 + 16;
    const float ListWidth = 720, ListBottom = 120;

    readonly string? folder;
    readonly Signal<string?> drive;
    readonly IKeyValue storage;
    readonly Action<PageKey> go;
    readonly Action<string, long> play;
    readonly Action<string> openDisc;
    readonly TrackListView list;
    readonly View message = new View();
    readonly TextLabel where;
    readonly IDisposable watch;
    IReadOnlyList<LocalEntry> entries = Array.Empty<LocalEntry>();
    int asked;
    bool gone;

    /// <param name="folder">The folder shown; null: the drive's top.</param>
    /// <param name="play">Plays a video, from a position.</param>
    /// <param name="openDisc">Opens a DVD's disc image: at its menus, else its titles.</param>
    public StorageScreen(TvShell shell, string? folder, Signal<string?> drive, IKeyValue storage, KeyRepeatGate gate, Action<PageKey> go, Action<string, long> play,
        Action<string> openDisc)
        : base(shell, new PageKey.Storage(folder), "storage")
    {
        this.folder = folder;
        this.drive = drive;
        this.storage = storage;
        this.go = go;
        this.play = play;
        this.openDisc = openDisc;
        var title = Text.Label(Pley.Tv.Core.Ui.Rail.StorageItem.Label, PleyType.PageTitle, PleyColors.Fg);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(title);
        where = Text.Label("", PleyType.Caption, PleyColors.Muted, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal);
        where.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical + 34 + 8));
        View.Add(where);
        list = new TrackListView(ListWidth, PleyTv.CanvasHeight - ListTop, gate, Pick, Tree.Focus, ListBottom, "storage-entries");
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
        // The drive pulled out, or another plugged in: the page shows what is there now.
        watch = drive.Watch(_ => Load());
    }

    /// <summary>The folder shown: the one asked for, else the drive's top.</summary>
    string? Folder => folder ?? drive.Value;

    public override void Enter()
    {
        // Back from the player: a video's line says where it resumes now.
        Lines();
        if (!Tree.Enter(list.Group)) Shell.FocusRail();
    }

    public override void Gone()
    {
        gone = true;
        watch.Dispose();
    }

    async void Load()
    {
        var at = ++asked;
        var dir = Folder;
        if (dir == null || drive.Value == null || !dir.StartsWith(drive.Value, StringComparison.Ordinal))
        {
            Show(Array.Empty<LocalEntry>(), "No USB drive", "Plug a USB drive into the TV to play its videos here.");
            return;
        }
        // Where it is on the drive: "USB drive › Films › 2019"; at its top, how full it is.
        var under = dir.Substring(drive.Value.Length).Trim('/');
        Text.Show(where, under.Length == 0 ? Space(dir) : string.Join(" › ", new[] { Pley.Tv.Core.Ui.Rail.StorageItem.Label }.Concat(under.Split('/'))));
        IReadOnlyList<LocalEntry> found;
        try
        {
            // A drive that sleeps takes a moment to answer: off the UI thread.
            found = await Task.Run(() => LocalFolder.List(dir));
        }
        catch (Exception)
        {
            if (at == asked && !gone) Show(Array.Empty<LocalEntry>(), "This folder can't be read", "The drive may have been pulled out.");
            return;
        }
        if (at != asked || gone) return;
        if (found.Count == 0) Show(found, "Nothing to play here", "This folder has no videos or folders in it.");
        else Show(found, null, null);
    }

    /// <summary>"437 GB free of 1.0 TB"; empty when the TV won't say.</summary>
    static string Space(string root)
    {
        try
        {
            var d = new DriveInfo(root);
            return $"{ItemFormat.Bytes(d.AvailableFreeSpace)} free of {ItemFormat.Bytes(d.TotalSize)}";
        }
        catch (Exception)
        {
            return "";
        }
    }

    void Show(IReadOnlyList<LocalEntry> found, string? title, string? body)
    {
        entries = found;
        foreach (var child in message.Children.ToList())
        {
            message.Remove(child);
            child.Dispose();
        }
        if (title != null)
        {
            var width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
            // Under the ".." line when there is one, where it was drawn over it.
            message.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(ListTop + (HasUp ? TrackLineView.IconLine + 24 : 40)));
            message.Add(Text.Label(title, PleyType.Title, PleyColors.Fg, width: width));
            var words = Text.Label(body ?? "", PleyType.Body, PleyColors.Muted, maxLines: 3, width: width);
            words.VerticalAlignment = VerticalAlignment.Top;
            words.PositionY = Ui.Dp(PleyType.Title.Line + 10);
            message.Add(words);
        }
        var hadFocus = list.Focused;
        Lines();
        if (Shell.Top != this) return;
        // The list keeps focus whenever it has a line (".." at least, under the drive's top); only an empty top gives it to the rail.
        var hasLines = list.Group.Children.Count > 0;
        if (hasLines && (Tree.Current == null || hadFocus || Tree.Current.Parent == Shell.Rail.Group && Shell.RootKey is PageKey.Storage)) Tree.Enter(list.Group);
        else if (!hasLines && hadFocus) Shell.FocusRail();
    }

    /// <summary>The line that goes up a folder, first in every folder under the drive's top.</summary>
    const string Up = "..";

    /// <summary>Whether the page has the ".." line: it's a folder under the drive's top.</summary>
    bool HasUp => folder != null && drive.Value != null && folder != drive.Value;

    /// <summary>
    /// The list's lines: ".." to go up; a folder its icon and name; a video its icon, name, kind
    /// ("MKV") or where it resumes with a bar of how far through it is, and its size at the end.
    /// </summary>
    void Lines()
    {
        var lines = new List<TrackLine>();
        if (HasUp)
            lines.Add(new TrackLine(Up, Up, Subtitle: "Up a folder", Icon: "corner-left-up"));
        lines.AddRange(entries.Select(e =>
        {
            if (e.Folder) return new TrackLine(e.Path, e.Name, Icon: "folder");
            if (e.Disc) return new TrackLine(e.Path, LocalFolder.Title(e.Path), Subtitle: "DVD", Duration: ItemFormat.Bytes(e.Size), Icon: "disc-3");
            var (resume, length) = LocalPlayerSource.Kept(storage, e.Path);
            var kind = System.IO.Path.GetExtension(e.Name).TrimStart('.').ToUpperInvariant();
            return new TrackLine(e.Path, LocalFolder.Title(e.Path),
                Subtitle: resume > 0 ? $"{kind} · Resumes at {PlayerText.Time(resume)}" : kind,
                Duration: ItemFormat.Bytes(e.Size),
                Icon: "file-video",
                Progress: resume > 0 && length > 0 ? Math.Max(0.02f, (float)resume / length) : 0);
        }));
        list.Set(lines);
    }

    void Pick(string path)
    {
        if (path == Up)
        {
            Shell.Back?.Invoke();
            return;
        }
        var entry = entries.FirstOrDefault(e => e.Path == path);
        if (entry == null) return;
        if (entry.Folder) go(new PageKey.Storage(entry.Path));
        else if (entry.Disc) openDisc(entry.Path);
        else play(entry.Path, LocalPlayerSource.ResumeOf(storage, entry.Path));
    }
}
