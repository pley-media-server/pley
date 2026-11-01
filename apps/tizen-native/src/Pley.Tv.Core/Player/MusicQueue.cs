using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Player;

/// <summary>The queue in the order it plays, the current track at <see cref="Index"/>.</summary>
public sealed record MusicQueueState(IReadOnlyList<ItemSummary> Tracks, int Index = 0, bool Shuffle = false)
{
    public static readonly MusicQueueState Empty = new MusicQueueState(Array.Empty<ItemSummary>());

    public ItemSummary? Current => Index >= 0 && Index < Tracks.Count ? Tracks[Index] : null;
}

/// <summary>What Previous did.</summary>
public enum PreviousMove
{
    /// <summary>The track starts again.</summary>
    Restart,
    /// <summary>The track before plays.</summary>
    Moved,
}

/// <summary>
/// The tracks to play and which one plays (the Android app's MusicQueue). The queue keeps the order
/// it was given in; shuffled, it plays a Fisher-Yates permutation of it that starts with the
/// current track, and unshuffled it goes back to the order given, at the same track. The same
/// track may be in it twice: positions, not ids, say which one plays.
/// </summary>
public sealed class MusicQueue
{
    /// <summary>Previous restarts a track that has played longer than this.</summary>
    public const long RestartAfterMs = 3_000;

    readonly Random random;
    IReadOnlyList<ItemSummary> given = Array.Empty<ItemSummary>();
    /// <summary>Play position → position in <see cref="given"/>.</summary>
    int[] order = new int[0];

    public MusicQueue(Random? random = null) => this.random = random ?? new Random();

    /// <summary>Where in the play order the current track is.</summary>
    public int Index { get; private set; }

    public bool Shuffle { get; private set; }

    public int Count => order.Length;

    public bool IsEmpty => order.Length == 0;

    /// <summary>The tracks in the order they play.</summary>
    public IReadOnlyList<ItemSummary> Tracks => order.Select(i => given[i]).ToList();

    public ItemSummary? Current => Index < order.Length ? given[order[Index]] : null;

    public bool HasNext => Index + 1 < order.Length;

    public MusicQueueState State => new MusicQueueState(Tracks, Index, Shuffle);

    /// <summary>Starts over with <paramref name="tracks"/> at <paramref name="startIndex"/>; shuffled, in a random order that begins with that track.</summary>
    public void Start(IReadOnlyList<ItemSummary> tracks, int startIndex = 0, bool shuffle = false)
    {
        given = tracks.ToList();
        order = Enumerable.Range(0, given.Count).ToArray();
        Index = given.Count == 0 ? 0 : Math.Max(0, Math.Min(startIndex, given.Count - 1));
        Shuffle = false;
        if (shuffle) SetShuffle(true);
    }

    /// <summary>Forgets every track.</summary>
    public void Clear() => Start(Array.Empty<ItemSummary>());

    /// <summary>Shuffles: the current track first, the rest after it in a random order. Unshuffles: the order given, at the same track.</summary>
    public void SetShuffle(bool on)
    {
        if (on == Shuffle || order.Length == 0)
        {
            Shuffle = on && order.Length > 0;
            return;
        }
        var playing = order[Index];
        if (on)
        {
            var rest = order.Where((_, i) => i != Index).ToArray();
            for (var i = rest.Length - 1; i >= 1; i--)
            {
                var j = random.Next(i + 1);
                (rest[i], rest[j]) = (rest[j], rest[i]);
            }
            order = new[] { playing }.Concat(rest).ToArray();
            Index = 0;
        }
        else
        {
            order = Enumerable.Range(0, given.Count).ToArray();
            Index = playing;
        }
        Shuffle = on;
    }

    /// <summary>Moves to the next track; false at the end.</summary>
    public bool Next()
    {
        if (!HasNext) return false;
        Index++;
        return true;
    }

    /// <summary>What Previous does (the web's rule): after 3 s of the track, or on the first track, it starts again; otherwise the track before plays.</summary>
    public PreviousMove Previous(long positionMs)
    {
        if (positionMs > RestartAfterMs || Index == 0) return PreviousMove.Restart;
        Index--;
        return PreviousMove.Moved;
    }

    /// <summary>Moves to the track at <paramref name="position"/> of the play order; false when there is none.</summary>
    public bool JumpTo(int position)
    {
        if (position < 0 || position >= order.Length) return false;
        Index = position;
        return true;
    }
}
