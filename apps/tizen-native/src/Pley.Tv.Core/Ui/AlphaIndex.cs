using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>
/// The A to Z of a listing sorted by title (the web's jump bar, apps/web/src/components/ItemGrid.tsx;
/// the Android app's AlphaIndex): for "#" and each letter, where its first title is. Titles go
/// under the first letter of their sort title (else title), accents taken off ("Élan" under E);
/// anything else, digits and other alphabets, under "#". Letters without titles are kept, so
/// screens can show them dimmed.
/// </summary>
public sealed class AlphaIndex : IEquatable<AlphaIndex>
{
    /// <summary>"#" then A to Z.</summary>
    public static readonly IReadOnlyList<string> Letters = new[] { "#" }.Concat(Enumerable.Range('A', 26).Select(c => ((char)c).ToString())).ToList();

    /// <summary>Listings this short don't need one (the web's 30).</summary>
    public const int MinItems = 30;

    /// <summary>For each of <see cref="Letters"/>, the position of its first title, or -1.</summary>
    readonly int[] firsts;
    /// <summary>Sections in listing order: their first positions and their letters, for <see cref="LetterAt"/>.</summary>
    readonly int[] starts;
    readonly int[] startLetters;

    AlphaIndex(int[] firsts, int[] starts, int[] startLetters)
    {
        this.firsts = firsts;
        this.starts = starts;
        this.startLetters = startLetters;
    }

    /// <summary>Whether <paramref name="letter"/> (an index into <see cref="Letters"/>) has titles.</summary>
    public bool Has(int letter) => PositionOf(letter) >= 0;

    /// <summary>Where <paramref name="letter"/>'s first title is; -1 without one.</summary>
    public int PositionOf(int letter) => letter >= 0 && letter < firsts.Length ? firsts[letter] : -1;

    /// <summary>Where to go for <paramref name="letter"/>: its first title, else the next letter's that has one, else the last letter's before it.</summary>
    public int JumpTarget(int letter)
    {
        for (var l = Math.Max(0, letter); l < Letters.Count; l++)
            if (firsts[l] >= 0) return firsts[l];
        for (var l = Math.Min(letter, Letters.Count - 1); l >= 0; l--)
            if (firsts[l] >= 0) return firsts[l];
        return 0;
    }

    /// <summary>The letter (an index into <see cref="Letters"/>) of the section <paramref name="position"/> is in.</summary>
    public int LetterAt(int position)
    {
        if (starts.Length == 0) return 0;
        if (position < starts[0]) return startLetters[0];
        var lo = 0;
        var hi = starts.Length - 1;
        while (lo < hi)
        {
            var mid = (lo + hi + 1) / 2;
            if (starts[mid] <= position) lo = mid;
            else hi = mid - 1;
        }
        return startLetters[lo];
    }

    /// <summary>The letters that have titles.</summary>
    public IReadOnlyList<int> Present => Enumerable.Range(0, Letters.Count).Where(Has).ToList();

    public bool Equals(AlphaIndex? other) => other != null && firsts.SequenceEqual(other.firsts);

    public override bool Equals(object? obj) => obj is AlphaIndex other && Equals(other);

    public override int GetHashCode() => firsts.Aggregate(17, (h, f) => h * 31 + f);

    /// <summary><paramref name="items"/>' index, in their order.</summary>
    public static AlphaIndex Of(IReadOnlyList<ItemSummary> items)
    {
        var firsts = Enumerable.Repeat(-1, Letters.Count).ToArray();
        var starts = new List<int>();
        var letters = new List<int>();
        var previous = -1;
        for (var position = 0; position < items.Count; position++)
        {
            var item = items[position];
            var letter = LetterOf(string.IsNullOrWhiteSpace(item.SortTitle) ? item.Title : item.SortTitle);
            if (firsts[letter] < 0) firsts[letter] = position;
            if (letter != previous)
            {
                starts.Add(position);
                letters.Add(letter);
                previous = letter;
            }
        }
        return new AlphaIndex(firsts, starts.ToArray(), letters.ToArray());
    }

    /// <summary>The letter <paramref name="title"/> goes under: 0 for "#", 1 to 26 for A to Z.</summary>
    public static int LetterOf(string title)
    {
        var t = title.Trim();
        if (t.Length == 0) return 0;
        // Half of a pair (an emoji, a letter past the first plane) can't be normalized alone; neither is A to Z.
        if (char.IsSurrogate(t[0])) return 0;
        var plain = char.ToUpperInvariant(t[0]).ToString().Normalize(NormalizationForm.FormD)
            .FirstOrDefault(c => CharUnicodeInfo.GetUnicodeCategory(c) != UnicodeCategory.NonSpacingMark);
        return plain is >= 'A' and <= 'Z' ? plain - 'A' + 1 : 0;
    }
}

/// <summary>
/// The A to Z jump beside the grid (design spec §4.2; the Android app's JumpControl): unfocused,
/// it shows the letter of the focused title's section. Focused (Right from the grid's last
/// column), Up and Down step through #, A to Z, with the letters before and after shown above and
/// below it, dimmed where no title starts with them; OK goes to the letter's first title (or the
/// next letter's that has one), and Up from "#" leaves upwards.
/// </summary>
public sealed class AlphaJump
{
    public AlphaJump(AlphaIndex index) => Index = index;

    /// <summary>The listing's A to Z; a new one keeps the letter stepped to.</summary>
    public AlphaIndex Index { get; set; }

    public bool Focused { get; private set; }

    /// <summary>The letter stepped to while focused (an index into <see cref="AlphaIndex.Letters"/>).</summary>
    public int Letter { get; private set; }

    /// <summary>The letter in the circle: the one stepped to while focused, else the section of the grid's focused position.</summary>
    public int Shown(int focusedPosition) => Focused ? Letter : Index.LetterAt(Math.Max(0, focusedPosition));

    /// <summary>The control takes focus: it starts at the focused title's letter.</summary>
    public void Focus(int focusedPosition)
    {
        if (!Focused) Letter = Index.LetterAt(Math.Max(0, focusedPosition));
        Focused = true;
    }

    public void Blur() => Focused = false;

    /// <summary>Up: the letter before. False at "#": focus leaves upwards.</summary>
    public bool Up()
    {
        if (Letter <= 0) return false;
        Letter--;
        return true;
    }

    /// <summary>Down: the letter after, staying on Z at the end (the key is used either way).</summary>
    public bool Down()
    {
        if (Letter < AlphaIndex.Letters.Count - 1) Letter++;
        return true;
    }

    /// <summary>Where OK goes: the position to scroll to and focus.</summary>
    public int Target => Index.JumpTarget(Letter);

    /// <summary>The letter shown above the circle while focused; null without one.</summary>
    public int? Above => Focused && Letter > 0 ? Letter - 1 : null;

    /// <summary>The letter shown below the circle while focused; null without one.</summary>
    public int? Below => Focused && Letter < AlphaIndex.Letters.Count - 1 ? Letter + 1 : null;

    /// <summary>What a screen reader says: "Jump to a letter: M".</summary>
    public string Description(int focusedPosition) => LibraryText.Jump(AlphaIndex.Letters[Shown(focusedPosition)]);
}
