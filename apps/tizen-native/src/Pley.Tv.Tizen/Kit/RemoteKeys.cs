namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The remote's keys beyond the D-pad, by the names NUI gives them on a Samsung TV (with a
/// keyboard's where one stands in for them, for the emulator).
/// </summary>
static class RemoteKeys
{
    public static bool ChannelUp(string key) => key is "XF86RaiseChannel" or "XF86ChannelUp" or "Prior";

    public static bool ChannelDown(string key) => key is "XF86LowerChannel" or "XF86ChannelDown" or "Next";

    public static bool Info(string key) => key is "XF86Info" or "F1";

    /// <summary>The Guide and Channel list keys.</summary>
    public static bool Guide(string key) => key is "XF86ChannelGuide" or "XF86ChannelList";

    public static bool Menu(string key) => key is "XF86SysMenu" or "XF86Menu" or "Menu";

    /// <summary>The remote's subtitles key (CC, or Samsung's SUBT.).</summary>
    public static bool Captions(string key) => key is "XF86Caption" or "XF86Subtitle" or "XF86ClosedCaption";

    /// <summary>Play, pause, rewind, fast forward and skip: what Live TV, watched as it airs, has none of.</summary>
    public static bool Transport(string key) => key is "XF86AudioPlay" or "XF86AudioPause" or "XF86AudioPlayPause" or "XF86AudioRewind" or "XF86AudioForward"
        or "XF86AudioNext" or "XF86AudioPrev";

    /// <summary>The digit a number key types; null for any other key.</summary>
    public static char? Digit(string key)
    {
        if (key.Length == 1 && key[0] >= '0' && key[0] <= '9') return key[0];
        if (key.Length == 4 && key.StartsWith("KP_", System.StringComparison.Ordinal) && key[3] >= '0' && key[3] <= '9') return key[3];
        return null;
    }
}
