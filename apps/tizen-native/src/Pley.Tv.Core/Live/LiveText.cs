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

    public LiveClock(CultureInfo? culture = null, TimeZoneInfo? zone = null)
    {
        this.culture = culture ?? CultureInfo.CurrentCulture;
        this.zone = zone ?? TimeZoneInfo.Local;
    }

    /// <summary>The TV's own: its culture and time zone.</summary>
    public static LiveClock Local => new LiveClock();

    DateTime At(long seconds) => TimeZoneInfo.ConvertTimeFromUtc(DateTimeOffset.FromUnixTimeSeconds(seconds).UtcDateTime, zone);

    /// <summary>"20:00", "8:00 PM".</summary>
    public string Time(long seconds) => At(seconds).ToString("t", culture);

    /// <summary>"20:00 – 21:00".</summary>
    public string Span(LiveProgram program) => $"{Time(program.Start)} – {Time(program.Stop)}";

    /// <summary>"Today", "Tomorrow", "Yesterday", or the date: "Fri 30 Oct".</summary>
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
    public string Next(LiveProgram next) => string.Format(CultureInfo.InvariantCulture, LiveText.NextFormat, Time(next.Start), next.Title);
}

/// <summary>What Live TV says (the Android app's strings_tv_livetv.xml), and how it says programmes' times.</summary>
public static class LiveText
{
    public const string Title = "Live TV";
    public const string Guide = "Guide";
    public const string Channels = "Channels";
    public const string Failed = "Live TV didn't load";
    public const string Retry = "Try again";
    public const string NotAllowedTitle = "Live TV isn't open to this profile";
    public const string NotAllowedBody = "An administrator can open it in Pley's web app, under Settings → Live TV.";
    public const string NoneTitle = "No channels yet";
    public const string NoneBody = "Channels appear here once an administrator adds an IPTV provider in Pley's web app.";
    public const string NoMatch = "No channels in this group.";
    public const string CategoryAll = "All channels";
    public const string CategoryFavorites = "Favourites";
    public const string CategoryRecent = "Recently watched";
    public const string Groups = "Groups";
    public const string Regions = "Regions";
    public const string MoreGroups = "More groups";
    public const string NoGroupName = "No group";
    public const string FoundGroups = "Groups found";
    public const string Search = "Search";
    public const string NoMatchTitle = "No channels match";
    public const string NoneFavoritesTitle = "No favourites yet";
    public const string NoneFavoritesBody = "Hold OK on a channel and add it to your favourites to find it here.";
    public const string NoneRecentTitle = "Nothing watched yet";
    public const string NoneRecentBody = "The channels you watch show here, the last first.";
    public const string NothingNow = "Nothing in the guide now";
    public const string NoGuide = "No guide for this channel";
    public const string NoInfo = "No information for this time";
    public const string Today = "Today";
    public const string Tomorrow = "Tomorrow";
    public const string Yesterday = "Yesterday";
    public const string BadgeLive = "Live";
    public const string BadgeNew = "New";
    public const string OnNow = "On now";
    public const string WatchNow = "Watch now";
    public const string WatchChannel = "Watch channel";
    public const string FavoriteAdd = "Add to favourites";
    public const string FavoriteRemove = "Remove from favourites";
    public const string NoDescription = "The guide says nothing more of it.";
    public const string Ended = "It has ended.";
    internal const string NextFormat = "Next {0} · {1}";

    // The live player.
    public const string Tuning = "Tuning…";
    public const string PlayerChannels = "Channels";
    public const string ChannelGroups = "Channel groups";
    public const string ChangeGroup = "Change group";
    public const string HeldByYou = "You, on another device";
    public const string TakeOver = "Watch here instead";
    public const string NoneToTake = "A channel already being watched can be joined, or try again once one is stopped.";
    public const string SearchChannels = "Search channels";
    public const string Live = "Live";
    public const string Radio = "Sound only";
    public const string Hint = "▲▼ change channel · OK for channels · hold OK for more";
    public const string PlayerRetry = "Try again";
    public const string OtherChannels = "Other channels";
    public const string BackToGuide = "Back to the guide";
    public const string Busy = "All connections are in use";
    public const string Stopped = "Watching stopped";
    public const string Missing = "This channel isn't here any more";
    public const string NotAllowed = "Live TV isn't open to this profile";
    public const string Offline = "Can't reach your server";

    /// <summary>
    /// The server answered the watch with an address that needs the token, which the TV's player
    /// can't send: an older Pley. Words of the web TV app's, which has the same limit; Android's
    /// player sends the token, so it has none.
    /// </summary>
    public const string OlderTitle = "Live TV needs a newer Pley on the server";
    public const string OlderBody = "This server can't send live channels to the TV's own player. Update Pley on the server, then try again.";

    /// <summary>"No channel's name, number or programme on now has “bbc” in it."</summary>
    public static string NoMatchSearch(string query) => $"No channel's name, number or programme on now has “{query.Trim()}” in it.";

    /// <summary>"The favourite wasn't saved: No answer in time."</summary>
    public static string FavoriteFailed(Exception e) => $"The favourite wasn't saved: {Errors.Message(e)}";

    /// <summary>"12 min left", "1 h left", "1 h 5 min left".</summary>
    public static string TimeLeft(LiveProgram program, long now)
    {
        var (hours, minutes) = LiveRules.Left(program, now);
        if (hours == 0) return $"{minutes} min left";
        return minutes == 0 ? $"{hours} h left" : $"{hours} h {minutes} min left";
    }

    /// <summary>"isn't playing": "BBC One isn't playing".</summary>
    public static string Unavailable(string channel) => $"{channel} isn't playing";

    /// <summary>"BBC One didn't start".</summary>
    public static string DidNotStart(string channel) => $"{channel} didn't start";

    /// <summary>"No channel 123".</summary>
    public static string NoChannel(string digits) => $"No channel {digits}";

    /// <summary>"Its one connection is watching:", "Its 2 connections are watching:".</summary>
    public static string ConnectionsWatching(int count) => count == 1 ? "Its one connection is watching:" : $"Its {count} connections are watching:";

    /// <summary>Who holds a connection: "You, on another device", their names (to administrators), or how many.</summary>
    public static string Holders(BusyConnection c)
    {
        if (c.Yours) return HeldByYou;
        if (c.Watchers.Count > 0) return string.Join(", ", c.Watchers);
        var people = Math.Max(1, c.People);
        return people == 1 ? "Someone else" : $"{people} people";
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

    /// <summary>A count beside a category: "1,234".</summary>
    public static string Count(int count) => count.ToString("N0", CultureInfo.InvariantCulture);

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
