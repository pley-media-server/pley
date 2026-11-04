using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Player;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The sample rates the TV takes, as the web TV app answers them too (packages/tv/test/fixtures/audio-rate.json).</summary>
public class AudioRateTests
{
    public static IEnumerable<object[]> PlaysCases =>
        Enumerable.Range(0, Fixtures.Load("audio-rate").GetProperty("plays").GetArrayLength()).Select(i => new object[] { i });

    static int? Number(JsonElement e) => e.ValueKind == JsonValueKind.Null ? null : e.GetInt32();

    [Theory]
    [MemberData(nameof(PlaysCases))]
    public void Plays_here(int i)
    {
        var c = Fixtures.Load("audio-rate").GetProperty("plays")[i];
        var a = c.GetProperty("in");
        var audio = new MediaStream { Index = 1, Kind = "audio", Codec = a.GetProperty("codec").GetString()!, Default = true, SampleRate = Number(a.GetProperty("sample_rate")) };
        var caps = new ClientCaps
        {
            AudioCodecs = new[] { "aac", "ac3", "eac3" }, AudioDecode = new[] { "aac", "ac3", "eac3" },
            AudioMaxSampleRate = Number(a.GetProperty("most")),
        };
        Assert.Equal(c.GetProperty("out").GetBoolean(), PlaybackPlan.AudioPlaysHere(audio, caps));
    }
}
