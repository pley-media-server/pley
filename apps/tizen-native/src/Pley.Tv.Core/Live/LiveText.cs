using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Live;

/// <summary>
/// Times of day as the TV writes them: "20:00" or "8:00 PM" by its culture, in its time zone
/// (the Android app's rememberClock and dayOf). Times are unix seconds.
/// </summary>
public sealed class LiveClock
{
    readonly CultureInfo culture;
    readonly TimeZoneInfo zone;

    /// <param name="culture">Times' and days' culture; the language shown's (<see cref="Strings.Culture"/>) without.</param>
    public LiveClock(CultureInfo? culture = null, TimeZoneInfo? zone = null)
    {
        this.culture = culture ?? Strings.Culture;
        this.zone = zone ?? TimeZoneInfo.Local;
    }

    /// <summary>The TV's own: the language shown's culture, and the TV's time zone.</summary>
    public static LiveClock Local => new LiveClock();

    DateTime At(long seconds) => TimeZoneInfo.ConvertTimeFromUtc(DateTimeOffset.FromUnixTimeSeconds(seconds).UtcDateTime, zone);

    /// <summary>"20:00", "8:00 PM".</summary>
    public string Time(long seconds) => At(seconds).ToString("t", culture);

    /// <summary>"20:00 – 21:00".</summary>
    public string Span(LiveProgram program) => $"{Time(program.Start)} – {Time(program.Stop)}";

    /// <summary>"Today", "Tomorrow", "Yesterday", or the date in the culture's words: "Fri 30 Oct", "ven. 30 oct.".</summary>
    public string Day(long seconds, long now)
    {
        var day = At(seconds).Date;
        var today = At(now).Date;
        if (day == today) return LiveText.Today;
        if (day == today.AddDays(1)) return LiveText.Tomorrow;
        if (day == today.AddDays(-1)) return LiveText.Yesterday;
        return At(seconds).ToString("ddd d MMM", culture);
    }

    /// <summary>Whether <paramref name="seconds"/> is midnight where the TV is: the guide's time header names the day there.</summary>
    public bool Midnight(long seconds)
    {
        var t = At(seconds);
        return t.Hour == 0 && t.Minute == 0;
    }

    /// <summary>"Next 21:00 · The News".</summary>
    public string Next(LiveProgram next) => Strings.T("livetv.guide.next", ("time", Time(next.Start)), ("title", next.Title));
}

/// <summary>
/// What Live TV says (the Android app's strings_tv_livetv.xml), in the language shown, and how it
/// says programmes' times.
/// </summary>
public static class LiveText
{
    public static string Title => Strings.T("livetv.title");
    public static string Guide => Strings.T("livetv.tabs.guide");
    public static string Channels => Strings.T("livetv.tabs.channels");
    public static string Failed => Strings.T("livetv.page.failed");
    public static string Retry => Strings.T("livetv.page.retry");
    public static string NotAllowedTitle => Strings.T("livetv.page.notAllowedTitle");
    public static string NotAllowedBody => Strings.T("livetv.page.notAllowedBody");
    public static string NoneTitle => Strings.T("livetv.page.noneTitle");
    public static string NoneBody => Strings.T("livetv.page.noneBody");
    public static string NoMatch => Strings.T("livetv.page.noMatch");
    public static string CategoryAll => Strings.T("livetv.categories.all");
    public static string CategoryFavorites => Strings.T("livetv.categories.favorites");
    public static string CategoryRecent => Strings.T("livetv.categories.recent");
    public static string Groups => Strings.T("livetv.categories.groups");
    public static string Regions => Strings.T("livetv.categories.regions");
    public static string MoreGroups => Strings.T("livetv.categories.moreGroups");
    public static string NoGroupName => Strings.T("livetv.categories.noGroup");
    public static string FoundGroups => Strings.T("livetv.categories.found");
    public static string Search => Strings.T("livetv.page.search");
    public static string NoMatchTitle => Strings.T("livetv.page.noMatchTitle");
    public static string NoneFavoritesTitle => Strings.T("livetv.page.noneFavoritesTitle");
    public static string NoneFavoritesBody => Strings.T("livetv.page.noneFavoritesBody");
    public static string NoneRecentTitle => Strings.T("livetv.page.noneRecentTitle");
    public static string NoneRecentBody => Strings.T("livetv.page.noneRecentBody");
    public static string NothingNow => Strings.T("livetv.guide.nothingNow");
    public static string NoGuide => Strings.T("livetv.guide.noGuide");
    public static string NoInfo => Strings.T("livetv.guide.noInfo");
    public static string Today => Strings.T("livetv.time.today");
    public static string Tomorrow => Strings.T("livetv.time.tomorrow");
    public static string Yesterday => Strings.T("livetv.time.yesterday");
    public static string BadgeLive => Strings.T("livetv.program.live");
    public static string BadgeNew => Strings.T("livetv.program.new");
    public static string OnNow => Strings.T("livetv.program.onNow");
    public static string WatchNow => Strings.T("livetv.program.watchNow");
    public static string WatchChannel => Strings.T("livetv.program.watchChannel");
    public static string FavoriteAdd => Strings.T("livetv.favorites.add");
    public static string FavoriteRemove => Strings.T("livetv.favorites.remove");
    public static string NoDescription => Strings.T("livetv.program.noDescription");
    public static string Ended => Strings.T("livetv.program.ended");

    // The live player.
    public static string Tuning => Strings.T("livetv.player.tuning");
    public static string PlayerChannels => Strings.T("livetv.player.channels");
    public static string ChannelGroups => Strings.T("livetv.player.channelGroups");
    public static string ChangeGroup => Strings.T("livetv.player.changeGroup");
    public static string HeldByYou => Strings.T("livetv.player.heldByYou");
    public static string TakeOver => Strings.T("livetv.player.takeOver");
    public static string NoneToTake => Strings.T("livetv.player.noneToTake");
    public static string SearchChannels => Strings.T("livetv.player.searchChannels");
    public static string Live => Strings.T("livetv.player.live");
    public static string Radio => Strings.T("livetv.player.radio");
    public static string Hint => Strings.T("livetv.player.hint");
    public static string PlayerRetry => Strings.T("livetv.player.retry");
    public static string OtherChannels => Strings.T("livetv.player.otherChannels");
    public static string BackToGuide => Strings.T("livetv.player.backToGuide");
    public static string Busy => Strings.T("livetv.trouble.busy");
    /// <summary>Watching ended on the server (its connection taken over, or stopped there): the server's words under it, else <see cref="StoppedBody"/>.</summary>
    public static string Stopped => Strings.T("livetv.trouble.stopped");
    public static string StoppedBody => Strings.T("livetv.trouble.stoppedBody");
    /// <summary>Watching stopped from elsewhere (the server's <c>session_stopped</c>): who stopped it, as Android says it.</summary>
    public static string StoppedByAdmin => Strings.T("livetv.trouble.stoppedByAdmin");
    public static string StoppedElsewhere => Strings.T("livetv.trouble.stoppedElsewhere");
    /// <summary>Try again's place when watching ended on the server: it tunes the channel as any watch does.</summary>
    public static string WatchAgain => Strings.T("livetv.player.watchAgain");
    public static string Missing => Strings.T("livetv.trouble.missing");
    public static string NotAllowed => Strings.T("livetv.trouble.notAllowed");
    public static string Offline => Strings.T("livetv.trouble.offline");

    /// <summary>
    /// The server answered the watch with an address that needs the token, which the TV's player
    /// can't send: an older Pley. Words of the web TV app's, which has the same limit; Android's
    /// player sends the token, so it has none.
    /// </summary>
    public static string OlderTitle => Strings.T("livetv.trouble.olderTitle");
    public static string OlderBody => Strings.T("livetv.trouble.olderBody");

    /// <summary>Why the TV's player gave up on a channel's HLS, which <see cref="PlayerSaid"/> shows once tuning again didn't help.</summary>
    public static string StartedLate => Strings.T("livetv.engine.late");
    public static string StreamEnded => Strings.T("livetv.engine.ended");
    public static string Stalled => Strings.T("livetv.engine.stalled");

    /// <summary>"The TV's player said: it stalled".</summary>
    public static string PlayerSaid(string detail) => Strings.T("livetv.engine.said", ("detail", detail));

    /// <summary>"No channel's name, number or programme on now has “bbc” in it."</summary>
    public static string NoMatchSearch(string query) => Strings.T("livetv.page.noMatchSearch", ("query", query.Trim()));

    /// <summary>"The favourite wasn't saved: No answer in time."</summary>
    public static string FavoriteFailed(Exception e) => Strings.T("livetv.favorites.failed", ("error", Errors.Message(e)));

    /// <summary>"12 min left", "1 h left", "1 h 5 min left".</summary>
    public static string TimeLeft(LiveProgram program, long now)
    {
        var (hours, minutes) = LiveRules.Left(program, now);
        if (hours == 0) return Strings.Count("livetv.time.minutesLeft", minutes);
        return minutes == 0 ? Strings.Count("livetv.time.hoursLeft", hours) : Strings.T("livetv.time.hoursMinutesLeft", ("hours", hours), ("minutes", minutes));
    }

    /// <summary>"isn't playing": "BBC One isn't playing".</summary>
    public static string Unavailable(string channel) => Strings.T("livetv.trouble.unavailable", ("channel", channel));

    /// <summary>"BBC One didn't start".</summary>
    public static string DidNotStart(string channel) => Strings.T("livetv.trouble.didNotStart", ("channel", channel));

    /// <summary>"No channel 123".</summary>
    public static string NoChannel(string digits) => Strings.T("livetv.player.noChannel", ("number", digits));

    /// <summary>"Its one connection is watching:", "Its 2 connections are watching:".</summary>
    public static string ConnectionsWatching(int count) => Strings.Count("livetv.player.connectionsWatching", count);

    /// <summary>Who holds a connection: "You, on another device", their names (to administrators), or how many.</summary>
    public static string Holders(BusyConnection c)
    {
        if (c.Yours) return HeldByYou;
        if (c.Watchers.Count > 0) return string.Join(", ", c.Watchers);
        var people = Math.Max(1, c.People);
        return people == 1 ? Strings.T("livetv.player.someoneElse") : Strings.Count("livetv.player.people", people);
    }

    /// <summary>Who stopped watching, then their message on its own line when they gave one (Android's session_stopped_message).</summary>
    public static string StoppedBy(string? message, bool byAdmin)
    {
        var who = byAdmin ? StoppedByAdmin : StoppedElsewhere;
        return string.IsNullOrWhiteSpace(message) ? who : who + "\n" + message!.Trim();
    }

    /// <summary>A trouble's title, as the player's card says it.</summary>
    public static string TroubleTitle(LiveTrouble trouble, string channel) => trouble.Kind switch
    {
        LiveTroubleKind.Busy => Busy,
        LiveTroubleKind.Stopped => Stopped,
        LiveTroubleKind.Missing => Missing,
        LiveTroubleKind.NotAllowed => NotAllowed,
        LiveTroubleKind.Unavailable => Unavailable(channel),
        LiveTroubleKind.Offline => Offline,
        LiveTroubleKind.Older => OlderTitle,
        _ => DidNotStart(channel),
    };

    /// <summary>A category's name as the player's channel list heads it: "All channels", "UK · Sport".</summary>
    public static string CategoryLabel(LiveCategory category) => category switch
    {
        LiveCategory.Favorites => CategoryFavorites,
        LiveCategory.Recent => CategoryRecent,
        LiveCategory.Group g => LiveRules.SplitRegion(g.Name) is { } split ? $"{split.Region} · {split.Label}" : g.Name,
        _ => CategoryAll,
    };

    /// <summary>A count beside a category, grouped as the language shown groups thousands: "1,234", "1 234".</summary>
    public static string Count(int count) => count.ToString("N0", Strings.Culture);

    /// <summary>The badge a programme carries, if any: Live first, else New.</summary>
    public static string? Badge(LiveProgram program) => program.IsLive ? BadgeLive : program.IsNew ? BadgeNew : null;

    /// <summary>A channel's number and name, two spaces apart, as the player's lists and cards say them: "101  BBC One".</summary>
    public static string NumberAndName(LiveChannel channel, string name) =>
        string.Join("  ", new[] { channel.Number, name }.Where(s => !string.IsNullOrEmpty(s)));

    /// <summary>What a channel's tile shows without a picture: its number, else the first three letters of its name; the name when <paramref name="name"/>.</summary>
    public static string LogoLabel(LiveChannel channel, bool name = false) =>
        name ? channel.Name : channel.Number ?? (channel.Name.Length > 3 ? channel.Name.Substring(0, 3) : channel.Name);

    /// <summary>What's on a channel's tile or row now: the title, else why there is none.</summary>
    public static string OnNowOf(LiveChannel channel, long now) => channel.OnAt(now)?.Title ?? (channel.HasGuide ? NothingNow : NoGuide);

    /// <summary>
    /// The guide's line under the title of what has focus: the channel (when a programme has
    /// focus), its day and times, the time left while it airs and the episode, "  ·  " apart;
    /// without a programme, the number and why there is none.
    /// </summary>
    public static string GuideMeta(LiveChannel channel, string name, LiveProgram? program, long now, LiveClock clock)
    {
        var parts = new List<string>();
        if (program != null)
        {
            parts.Add(NumberOrName(channel, name));
            parts.Add($"{clock.Day(program.Start, now)} {clock.Span(program)}");
            if (program.AiringAt(now)) parts.Add(TimeLeft(program, now));
            if (program.EpisodeLabel is { } episode) parts.Add(episode);
        }
        else
        {
            if (channel.Number != null) parts.Add(channel.Number);
            parts.Add(channel.HasGuide ? NoInfo : NoGuide);
        }
        return string.Join("  ·  ", parts);
    }

    static string NumberOrName(LiveChannel channel, string name) => string.Join(" ", new[] { channel.Number, name }.Where(s => !string.IsNullOrEmpty(s)));

    /// <summary>The guide's text under the meta line: the subtitle and the description, " — " apart; null with neither.</summary>
    public static string? GuideText(LiveProgram? program)
    {
        if (program == null) return null;
        var text = string.Join(" — ", new[] { program.Subtitle, program.Description }.Where(s => !string.IsNullOrEmpty(s)));
        return text.Length > 0 ? text : null;
    }

    /// <summary>What a programme cell says to a screen reader: "The News, 20:00 – 21:00".</summary>
    public static string Spoken(LiveProgram program, LiveClock clock) => $"{program.Title}, {clock.Span(program)}";
}
