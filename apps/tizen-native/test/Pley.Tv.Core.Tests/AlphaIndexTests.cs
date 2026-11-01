using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The A to Z of a listing by title, as the web's jump bar works it out: the Android app's AlphaIndexTest, and its jump control.</summary>
public class AlphaIndexTests
{
    static ItemSummary Item(long id, string title, string sortTitle = "") => new ItemSummary { Id = id, LibraryId = 1, Kind = "movie", Title = title, SortTitle = sortTitle };

    static int Letter(string l) => AlphaIndex.Letters.ToList().IndexOf(l);

    [Fact]
    public void Letters_come_from_the_sort_title_without_accents()
    {
        Assert.Equal(Letter("E"), AlphaIndex.LetterOf("Élan"));
        Assert.Equal(Letter("O"), AlphaIndex.LetterOf("  Öresund"));
        Assert.Equal(Letter("A"), AlphaIndex.LetterOf("ångström"));
        Assert.Equal(Letter("C"), AlphaIndex.LetterOf("Ça ira"));
        Assert.Equal(Letter("#"), AlphaIndex.LetterOf("2001: A Space Odyssey"));
        Assert.Equal(Letter("#"), AlphaIndex.LetterOf("Ωmega"));
        Assert.Equal(Letter("#"), AlphaIndex.LetterOf("!Women Art Revolution"));
        Assert.Equal(Letter("#"), AlphaIndex.LetterOf(""));
        Assert.Equal(Letter("#"), AlphaIndex.LetterOf("😀 Smile"));
        // "The Matrix" sorts as "Matrix".
        var index = AlphaIndex.Of(new[] { Item(1, "The Matrix", "Matrix") });
        Assert.True(index.Has(Letter("M")));
        Assert.False(index.Has(Letter("T")));
    }

    [Fact]
    public void Each_letter_knows_its_first_title()
    {
        var index = AlphaIndex.Of(new[]
        {
            Item(1, "12 Monkeys"),
            Item(2, "Alien"),
            Item(3, "Aliens"),
            Item(4, "Élite Squad"),
            Item(5, "Heat"),
            Item(6, "Zodiac"),
        });
        Assert.Equal(0, index.PositionOf(Letter("#")));
        Assert.Equal(1, index.PositionOf(Letter("A")));
        Assert.Equal(3, index.PositionOf(Letter("E")));
        Assert.Equal(5, index.PositionOf(Letter("Z")));
        // Empty letters are kept, to be shown dimmed.
        Assert.False(index.Has(Letter("B")));
        Assert.Equal(-1, index.PositionOf(Letter("B")));
        Assert.Equal(new[] { "#", "A", "E", "H", "Z" }, index.Present.Select(l => AlphaIndex.Letters[l]));
    }

    [Fact]
    public void An_empty_letter_jumps_to_the_next_that_has_titles()
    {
        var index = AlphaIndex.Of(new[] { Item(1, "Alien"), Item(2, "Heat"), Item(3, "Up") });
        Assert.Equal(1, index.JumpTarget(Letter("B")));
        Assert.Equal(2, index.JumpTarget(Letter("U")));
        // Past the last: the last letter's.
        Assert.Equal(2, index.JumpTarget(Letter("Z")));
    }

    [Fact]
    public void A_position_is_in_its_letters_section()
    {
        var index = AlphaIndex.Of(new[] { Item(1, "#1"), Item(2, "Alien"), Item(3, "Aliens"), Item(4, "Heat"), Item(5, "Hero") });
        Assert.Equal(Letter("#"), index.LetterAt(0));
        Assert.Equal(Letter("A"), index.LetterAt(1));
        Assert.Equal(Letter("A"), index.LetterAt(2));
        Assert.Equal(Letter("H"), index.LetterAt(4));
        Assert.Equal(Letter("H"), index.LetterAt(400));
    }

    [Fact]
    public void The_jump_steps_through_the_letters_from_the_focused_titles()
    {
        var index = AlphaIndex.Of(new[] { Item(1, "Alien"), Item(2, "Brazil"), Item(3, "Heat"), Item(4, "Up") });
        var jump = new AlphaJump(index);
        // Unfocused, it follows the grid.
        Assert.Equal(Letter("H"), jump.Shown(2));
        Assert.Null(jump.Above);
        jump.Focus(1);
        Assert.Equal(Letter("B"), jump.Letter);
        Assert.Equal(Letter("A"), jump.Above);
        Assert.Equal(Letter("C"), jump.Below);
        Assert.True(jump.Down());
        // C has no titles: OK goes to the next letter that has some.
        Assert.False(index.Has(jump.Letter));
        Assert.Equal(2, jump.Target);
        Assert.Equal("Jump to a letter: C", jump.Description(0));
        Assert.True(jump.Up());
        Assert.True(jump.Up());
        Assert.True(jump.Up());
        Assert.Equal(0, jump.Letter);
        // Up from "#" leaves upwards.
        Assert.False(jump.Up());
        jump.Blur();
        Assert.Equal(Letter("U"), jump.Shown(3));
        // The jump scrolls to its row's start, then focuses the title.
        Assert.Equal(14, LibraryRules.RowStart(17, 7));
    }

    [Fact]
    public void The_grid_has_seven_posters_a_row_and_five_squares()
    {
        Assert.Equal(7, LibraryRules.GridColumns(TileKind.LibraryGrid));
        Assert.Equal(5, LibraryRules.GridColumns(TileKind.Square));
    }
}
