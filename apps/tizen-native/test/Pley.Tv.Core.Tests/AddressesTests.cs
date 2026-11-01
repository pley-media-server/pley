using System.Collections.Generic;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>Server addresses answer as the web TV app's do (fixtures/addresses.json).</summary>
public class AddressesTests
{
    public static IEnumerable<object[]> NormalizeServerCases => Fixtures.Cases("addresses", "normalizeServer");
    public static IEnumerable<object[]> CandidatesCases => Fixtures.Cases("addresses", "candidates");
    public static IEnumerable<object[]> IsLocalHostCases => Fixtures.Cases("addresses", "isLocalHost");
    public static IEnumerable<object[]> AtHomeCases => Fixtures.Cases("addresses", "atHome");
    public static IEnumerable<object[]> UnencryptedCases => Fixtures.Cases("addresses", "unencrypted");
    public static IEnumerable<object[]> DisplayAddressCases => Fixtures.Cases("addresses", "displayAddress");
    public static IEnumerable<object[]> OriginCases => Fixtures.Cases("addresses", "origin");

    [Theory, MemberData(nameof(NormalizeServerCases))]
    public void NormalizeServer(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.NormalizeServer(input)));

    [Theory, MemberData(nameof(CandidatesCases))]
    public void Candidates(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.Candidates(input)));

    [Theory, MemberData(nameof(IsLocalHostCases))]
    public void IsLocalHost(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.IsLocalHost(input)));

    [Theory, MemberData(nameof(AtHomeCases))]
    public void AtHome(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.AtHome(input)));

    [Theory, MemberData(nameof(UnencryptedCases))]
    public void Unencrypted(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.IsUnencrypted(input)));

    [Theory, MemberData(nameof(DisplayAddressCases))]
    public void DisplayAddress(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.DisplayAddress(input)));

    [Theory, MemberData(nameof(OriginCases))]
    public void Origin(string input, string expected) =>
        Assert.Equal(Fixtures.Normalize(expected), Fixtures.Json(Addresses.Origin(input)));

    // Where the Android app reads addresses with Java's URI, beyond the shared fixture.
    [Theory]
    [InlineData("me@nas", "http://nas:7539")]
    [InlineData("https://me:secret@pley.example.com/", "https://pley.example.com")]
    [InlineData("my_nas", null)]
    [InlineData("http://my_nas:7539", null)]
    [InlineData("nås", null)]
    public void Reads_addresses_as_the_android_app(string input, string? expected) =>
        Assert.Equal(expected, Addresses.NormalizeServer(input));

    // The Android app's LocalRoute.isHomeHost: this device and Tailscale aren't the home network.
    [Theory]
    [InlineData("http://localhost:7539", false)]
    [InlineData("http://127.0.0.1:7539", false)]
    [InlineData("http://[::1]:7539", false)]
    [InlineData("http://10.0.0.5:7539", true)]
    [InlineData("http://172.16.4.1:7539", true)]
    [InlineData("http://169.254.3.3:7539", true)]
    [InlineData("http://[fe80::1]:7539", true)]
    [InlineData("http://[fd12::1]:7539", true)]
    [InlineData("http://[2001:db8::1]:7539", false)]
    [InlineData("http://nas.local:7539", true)]
    [InlineData("http://8.8.8.8:7539", false)]
    public void Knows_the_home_network(string url, bool expected) => Assert.Equal(expected, Addresses.AtHome(url));
}
