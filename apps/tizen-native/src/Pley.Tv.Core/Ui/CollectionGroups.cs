using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Ui;

/// <summary>The Collections tab's Category filter: every one, or one category by its id.</summary>
public abstract record CategoryChoice
{
    public sealed record All : CategoryChoice;

    public sealed record Of(long Id) : CategoryChoice;

    /// <summary>A choice as a panel row's value: "all" or the category's id.</summary>
    public string Value => this switch
    {
        Of of => of.Id.ToString(CultureInfo.InvariantCulture),
        _ => "all",
    };

    /// <summary>The choice of a panel row's <see cref="Value"/>; every one for what it doesn't know.</summary>
    public static CategoryChoice Parse(string? value) => value switch
    {
        _ when long.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out var id) => new Of(id),
        _ => new All(),
    };
}

/// <summary>Collections of one category (the server's Other for those without one).</summary>
public sealed record CollectionGroup(CategoryRef Category, IReadOnlyList<CollectionTile> Collections)
{
    /// <summary>The key of its row: the category's id.</summary>
    public string Key => Category.Id.ToString(CultureInfo.InvariantCulture);

    /// <summary>The choice that picks it.</summary>
    public CategoryChoice Choice => new CategoryChoice.Of(Category.Id);

    /// <summary>Whether <paramref name="choice"/> picks it.</summary>
    public bool IsChosen(CategoryChoice choice) => choice is CategoryChoice.All || choice == Choice;
}

/// <summary>
/// A library's collections as the Collections tab shows them (apps/web's CollectionsTab; the
/// Android app's CollectionsView): those of the <see cref="Category"/> chosen, by category.
/// <c>packages/tv/test/fixtures/collections.json</c> pins it for both Samsung apps.
/// </summary>
/// <param name="Choices">The categories that have collections, offered in the Category filter.</param>
/// <param name="Category">The category chosen: one that has no collections any more chooses every one.</param>
/// <param name="Total">How many collections the library has, whatever the filter.</param>
public sealed record CollectionsView(IReadOnlyList<CollectionGroup> Groups, IReadOnlyList<CollectionGroup> Choices, CategoryChoice Category, int Total)
{
    public static readonly CollectionsView Empty = new CollectionsView(new CollectionGroup[0], new CollectionGroup[0], new CategoryChoice.All(), 0);

    /// <summary>How many collections show.</summary>
    public int Count => Groups.Sum(g => g.Collections.Count);

    /// <summary>The groups have headings (a row each on TV): not when only one shows.</summary>
    public bool Headed => Groups.Count > 1;

    /// <summary>A category is chosen.</summary>
    public bool Filtered => Category is not CategoryChoice.All;

    /// <summary>The Category filter is offered: more than one category to choose from, or one chosen.</summary>
    public bool Offered => Choices.Count > 1 || Filtered;

    /// <summary>The chosen group, for the chip's title; null with every one chosen.</summary>
    public CollectionGroup? Chosen => Filtered ? Choices.FirstOrDefault(g => g.Choice == Category) : null;

    /// <summary><paramref name="collections"/> of the category <paramref name="chosen"/>, grouped by <paramref name="categories"/> in their order.</summary>
    public static CollectionsView Of(IReadOnlyList<CollectionTile> collections, IReadOnlyList<CollectionCategory> categories, CategoryChoice chosen)
    {
        var choices = GroupByCategory(collections, categories);
        var category = chosen is CategoryChoice.All || choices.Any(g => g.Choice == chosen) ? chosen : new CategoryChoice.All();
        var groups = choices.Where(g => g.IsChosen(category)).ToList();
        return new CollectionsView(groups, choices, category, collections.Count);
    }

    /// <summary>
    /// <paramref name="collections"/> by the category the server gives each (its Other for those
    /// without one), in the order of <paramref name="categories"/>; empty groups left out. A
    /// collection of a category not listed (made since the list was read) comes after the rest,
    /// under its own category.
    /// </summary>
    public static IReadOnlyList<CollectionGroup> GroupByCategory(IReadOnlyList<CollectionTile> collections, IReadOnlyList<CollectionCategory> categories)
    {
        var order = categories.Select(c => new CategoryRef(c.Id, c.Key, c.Title)).ToList();
        var members = order.ToDictionary(c => c.Id, _ => new List<CollectionTile>());
        foreach (var x in collections)
        {
            // A server older than categories names none: its collections make one group, shown without a heading.
            var category = x.Category ?? None;
            if (!members.TryGetValue(category.Id, out var list))
            {
                list = new List<CollectionTile>();
                members[category.Id] = list;
                order.Add(category);
            }
            list.Add(x);
        }
        return order.Select(c => new CollectionGroup(c, members[c.Id])).Where(g => g.Collections.Count > 0).ToList();
    }

    static readonly CategoryRef None = new CategoryRef(0, null, "");
}
