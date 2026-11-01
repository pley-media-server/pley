using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;

namespace Pley.Tv.Core.Player;

/// <summary>The player's side panels (design spec §2.7), opened from the control row.</summary>
public enum PlayerPanelKind
{
    Subtitles,
    Audio,
    Quality,
    Chapters,
    /// <summary>Playback details: the readout, which Up and Down scroll; nothing to choose.</summary>
    Info,
    Sleep,
    Speed,
}

public enum PlayerRowKind
{
    /// <summary>A muted heading over the rows after it: "Subtitle size", "Video".</summary>
    Heading,
    /// <summary>A muted sentence, not focusable: why it plays lower, "Asking the server…".</summary>
    Note,
    /// <summary>A row OK chooses (<see cref="PlayerModel.Choose"/>), ticked when it is the current choice.</summary>
    Choice,
    /// <summary>
    /// A value Left and Right step (<see cref="PlayerModel.Step"/>) and OK resets: Sync. Its
    /// <see cref="PlayerRow.Value"/> shows between chevrons, its <see cref="PlayerRow.Supporting"/> under the label.
    /// </summary>
    Step,
    /// <summary>A line of the readout: a muted label and its value.</summary>
    Fact,
}

/// <summary>What choosing a row does.</summary>
public abstract record PlayerRowAction
{
    /// <summary>Null: off.</summary>
    public sealed record Subtitle(int? Index) : PlayerRowAction;
    public sealed record Audio(int Index) : PlayerRowAction;
    public sealed record Quality(string Id) : PlayerRowAction;
    public sealed record PlayOriginal : PlayerRowAction;
    public sealed record Chapter(long StartMs) : PlayerRowAction;
    public sealed record Sleep(SleepChoice Choice) : PlayerRowAction;
    public sealed record Speed(double Rate) : PlayerRowAction;
    /// <summary>The Sync row: Left and Right move the subtitles 0.1 s, OK puts them back.</summary>
    public sealed record SubtitleSync : PlayerRowAction;
    public sealed record SubtitleSize(float Size) : PlayerRowAction;
    public sealed record SubtitleBackground(Core.SubtitleBackground Background) : PlayerRowAction;
    public sealed record SubtitleFont(Core.SubtitleFont Font) : PlayerRowAction;
    public sealed record SubtitleRaise(float Raise) : PlayerRowAction;
}

/// <summary>A row of a player panel.</summary>
/// <param name="Supporting">The line under the label: a quality's hint, a chapter's time, "42 min left", Sync's "OK resets".</param>
/// <param name="Value">A step row's value ("+0.5 s"), or a fact's.</param>
/// <param name="Icon">Lucide's name: "rotate-ccw" for Play original.</param>
public sealed record PlayerRow(PlayerRowKind Kind, string Label, bool Checked = false, string? Supporting = null, string? Value = null, string? Icon = null, PlayerRowAction? Action = null);

/// <summary>
/// A player panel: its title, its rows, and the row focus starts on (the current choice, else the
/// first choice). 360 dp wide, Info 440 dp with a 112 dp label column.
/// </summary>
public sealed record PlayerPanel(PlayerPanelKind Kind, string Title, IReadOnlyList<PlayerRow> Rows, int Focus)
{
    public const float Width = 360, InfoWidth = 440, InfoLabelWidth = 112;

    public bool Wide => Kind == PlayerPanelKind.Info;
}

/// <summary>What the control row offers beyond playing and seeking (the Android app's RowOptions).</summary>
/// <param name="PreviousNext">Previous and Next: an episode, or a file with chapters.</param>
/// <param name="Speed">Unlike Android's, only when the TV's player plays other speeds with their sound.</param>
public sealed record RowOptions(bool PreviousNext, bool Subtitles, bool Audio, bool Quality, bool Chapters, bool Speed)
{
    public static RowOptions Of(PlayerState s) => new RowOptions(
        PreviousNext: s.Subtitle != null || s.Chapters.Count > 1,
        Subtitles: s.Subtitles.Count > 0,
        // One track still has its panel, as on Android, where its delay is.
        Audio: s.Audio.Count > 0,
        Quality: s.Qualities.Count > 0,
        Chapters: s.Chapters.Count > 1,
        Speed: s.Speeds.Count > 0);
}

/// <summary>
/// The player's panels as data (the Android app's TvPlayerPanels): what each lists, what is
/// ticked, and where focus starts. Choosing closes the panel, but for the subtitles' look and the
/// Sync row, which change what shows behind it.
/// </summary>
public static class PlayerPanels
{
    public const string SubtitleSize = "Subtitle size";
    public const string SubtitleBackground = "Subtitle background";
    public const string SubtitleFont = "Subtitle font";
    public const string SubtitlePosition = "Subtitle position";

    /// <summary>Settings' subtitle sizes' names, in <see cref="Settings.SubtitleSizes"/>' order.</summary>
    public static readonly IReadOnlyList<string> SizeLabels = new[] { "Small", "Normal", "Large", "Huge" };

    /// <summary>Settings' positions' names, in <see cref="Settings.SubtitleRaises"/>' order.</summary>
    public static readonly IReadOnlyList<string> PositionLabels = new[] { "Bottom", "Raised", "Higher", "Highest" };

    public static string BackgroundLabel(Core.SubtitleBackground background) => background == Core.SubtitleBackground.DarkBox ? "Dark box" : "None";

    public static string FontLabel(Core.SubtitleFont font) => font switch
    {
        Core.SubtitleFont.Serif => "Serif",
        Core.SubtitleFont.Monospace => "Monospace",
        Core.SubtitleFont.Casual => "Casual",
        _ => "Sans serif",
    };

    /// <summary>The panel's title, as the control row's button names it.</summary>
    public static string Title(PlayerPanelKind kind) => kind switch
    {
        PlayerPanelKind.Subtitles => PlayerText.Subtitles,
        PlayerPanelKind.Audio => PlayerText.Audio,
        PlayerPanelKind.Quality => PlayerText.Quality,
        PlayerPanelKind.Chapters => PlayerText.Chapters,
        PlayerPanelKind.Info => PlayerText.InfoTitle,
        PlayerPanelKind.Sleep => PlayerText.Sleep,
        _ => PlayerText.Speed,
    };

    /// <summary>
    /// The panel <paramref name="kind"/> for <paramref name="state"/>, the playhead at
    /// <paramref name="positionMs"/> (the chapter playing is ticked). The subtitles' look is
    /// <paramref name="settings"/>'; <paramref name="fonts"/> the fonts this TV has (every one without).
    /// Info lists <paramref name="readout"/>'s sections.
    /// </summary>
    public static PlayerPanel Build(PlayerPanelKind kind, PlayerState state, long positionMs, Settings settings, ReadoutInput? readout = null, IReadOnlyList<Core.SubtitleFont>? fonts = null)
    {
        var rows = new List<PlayerRow>();
        int? focus = null;
        void Choices(IEnumerable<PlayerRow> choices)
        {
            var start = rows.Count;
            rows.AddRange(choices);
            if (focus != null || rows.Count == start) return;
            var ticked = rows.FindIndex(start, r => r.Checked);
            focus = ticked >= 0 ? ticked : start;
        }
        switch (kind)
        {
            case PlayerPanelKind.Subtitles:
                if (state.SubtitleSync)
                    rows.Add(new PlayerRow(PlayerRowKind.Step, PlayerText.Sync, Supporting: PlayerText.SyncHint, Value: Player.SubtitleSync.Label(state.SubtitleOffsetMs), Action: new PlayerRowAction.SubtitleSync()));
                Choices(state.Subtitles.Select(t => new PlayerRow(PlayerRowKind.Choice, t.Label, t.Selected, Action: new PlayerRowAction.Subtitle(t.Index))));
                if (state.Subtitles.Count > 0) Look(rows, settings, fonts);
                break;
            case PlayerPanelKind.Audio:
                Choices(state.Audio.Where(t => t.Index != null).Select(t => new PlayerRow(PlayerRowKind.Choice, t.Label, t.Selected, Action: new PlayerRowAction.Audio(t.Index!.Value))));
                break;
            case PlayerPanelKind.Quality:
                if (state.Note != null) rows.Add(new PlayerRow(PlayerRowKind.Note, state.Note));
                if (state.CanPlayOriginal)
                    rows.Add(new PlayerRow(PlayerRowKind.Choice, PlayerText.PlayOriginal, Supporting: PlayerText.PlayOriginalHint, Icon: "rotate-ccw", Action: new PlayerRowAction.PlayOriginal()));
                Choices(state.Qualities.Select(q => new PlayerRow(PlayerRowKind.Choice, q.Label, q.Selected, q.Hint, Action: new PlayerRowAction.Quality(q.Id))));
                break;
            case PlayerPanelKind.Chapters:
                var playing = PlayerText.ChapterAt(state.Chapters, positionMs);
                Choices(state.Chapters.Select((c, i) => new PlayerRow(PlayerRowKind.Choice, PlayerText.ChapterTitle(c, i), i == playing, PlayerText.Time(c.StartMs), Action: new PlayerRowAction.Chapter(c.StartMs))));
                break;
            case PlayerPanelKind.Sleep:
                Choices(SleepChoice.All.Select(choice =>
                {
                    var on = state.Sleep.Set == choice;
                    var left = on && choice is SleepChoice.Minutes && state.Sleep.MinutesLeft != null ? SleepText.Summary(state.Sleep) : null;
                    return new PlayerRow(PlayerRowKind.Choice, SleepText.Choice(choice), on, left, Action: new PlayerRowAction.Sleep(choice));
                }));
                break;
            case PlayerPanelKind.Speed:
                Choices(PlaybackSpeeds.All.Where(r => r == 1 || state.Speeds.Contains(r)).Select(r => new PlayerRow(PlayerRowKind.Choice, PlaybackSpeeds.Label(r), r == state.Speed, Action: new PlayerRowAction.Speed(r))));
                break;
            case PlayerPanelKind.Info:
                var sections = readout != null ? Readout.Sections(readout) : new MediaFactBlock[0];
                if (readout?.Details == null) rows.Add(new PlayerRow(PlayerRowKind.Note, PlayerText.InfoWaiting));
                foreach (var section in sections)
                {
                    rows.Add(new PlayerRow(PlayerRowKind.Heading, section.Title));
                    rows.AddRange(section.Facts.Select(f => new PlayerRow(PlayerRowKind.Fact, f.Label, Value: f.Value)));
                }
                break;
        }
        return new PlayerPanel(kind, Title(kind), rows, focus ?? 0);
    }

    /// <summary>The subtitles' size, background, font and position under the tracks, as Settings has them (and saved there).</summary>
    static void Look(List<PlayerRow> rows, Settings settings, IReadOnlyList<Core.SubtitleFont>? fonts)
    {
        var sizes = Settings.SubtitleSizes;
        var size = Nearest(sizes, settings.SubtitleSize);
        rows.Add(new PlayerRow(PlayerRowKind.Heading, SubtitleSize));
        rows.AddRange(sizes.Select((s, i) => new PlayerRow(PlayerRowKind.Choice, SizeLabels[i], i == size, Action: new PlayerRowAction.SubtitleSize(s))));
        rows.Add(new PlayerRow(PlayerRowKind.Heading, SubtitleBackground));
        foreach (var background in new[] { Core.SubtitleBackground.None, Core.SubtitleBackground.DarkBox })
            rows.Add(new PlayerRow(PlayerRowKind.Choice, BackgroundLabel(background), settings.SubtitleBackground == background, Action: new PlayerRowAction.SubtitleBackground(background)));
        rows.Add(new PlayerRow(PlayerRowKind.Heading, SubtitleFont));
        foreach (var font in fonts ?? (IReadOnlyList<Core.SubtitleFont>)Enum.GetValues(typeof(Core.SubtitleFont)).Cast<Core.SubtitleFont>().ToList())
            rows.Add(new PlayerRow(PlayerRowKind.Choice, FontLabel(font), settings.SubtitleFont == font, Action: new PlayerRowAction.SubtitleFont(font)));
        var raises = Settings.SubtitleRaises;
        var raise = Nearest(raises, settings.SubtitleRaise);
        rows.Add(new PlayerRow(PlayerRowKind.Heading, SubtitlePosition));
        rows.AddRange(raises.Select((r, i) => new PlayerRow(PlayerRowKind.Choice, PositionLabels[i], i == raise, Action: new PlayerRowAction.SubtitleRaise(r))));
    }

    static int Nearest(IReadOnlyList<float> values, float value)
    {
        var best = 0;
        for (var i = 1; i < values.Count; i++)
            if (Math.Abs(values[i] - value) < Math.Abs(values[best] - value)) best = i;
        return best;
    }
}
