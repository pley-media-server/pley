using System;

namespace Pley.Tv.Core.Player;

/// <summary>
/// What a TV's player that says little is doing, from its position read again and again: Tizen's
/// player says neither when its first picture is out nor when it waits for data, only where it is
/// (its buffering percentage isn't a wait: a stream plays on under 100 %). For one source, from its
/// prepare to its end; the engine makes the player's calls, reads the position off the UI thread
/// and hands each read in here, and says what this decides.
/// <list type="bullet">
/// <item>Its status (<see cref="EngineStatus"/>): waiting until the first picture moves when it is
/// to play, as after a seek until it moves on from where it landed, and when it stands still
/// <see cref="StillMs"/> while it should play; ready otherwise; ended when the player says so.</item>
/// <item>Where it is on the film's timeline: a trimmed playlist (<see cref="StartWindow"/>) has the
/// player count from the window's start, added back to every position and seek; between reads the
/// position runs on, <see cref="ProjectMs"/> at most; a read asked before a seek is dropped.</item>
/// <item>How often to read: often until the picture moves and while a seek settles, so the
/// screen's cover and spinner go as soon as it does; every half second otherwise.</item>
/// </list>
/// Its events come at once, from the call that caused them; the UI thread only.
/// </summary>
public sealed class PlaybackWatch
{
    /// <summary>How far the position moves on before it counts as moving: its first read after a start or a seek is where it landed, not a move.</summary>
    public const long FirstMoveMs = 80;
    /// <summary>Standing still this long while it should play: waiting for data.</summary>
    public const long StillMs = 1_500;
    /// <summary>The most the position runs on past the last read: no further, while a read is late or the player waits.</summary>
    public const long ProjectMs = 500;
    /// <summary>How often the position is read until the first picture moves.</summary>
    public const uint StartEveryMs = 40;
    /// <summary>How often while a seek settles.</summary>
    public const uint SettleEveryMs = 100;
    /// <summary>How often otherwise.</summary>
    public const uint EveryMs = 500;

    readonly Func<long> now;
    bool opened, wanted, moved, settling, waiting, ended;
    long last, readAt;
    long? pending;
    long firstAt = -1, settleFrom = -1, watchedAt = -1, stillSince;
    long shift;
    bool shiftKnown;

    /// <param name="startMs">Where the source starts on the film's timeline: the position until the player says.</param>
    /// <param name="now">A clock in ms.</param>
    public PlaybackWatch(long startMs, bool play, Func<long> now)
    {
        this.now = now;
        last = Math.Max(0, startMs);
        wanted = play;
    }

    /// <summary>The status changed.</summary>
    public event Action<EngineStatus>? StatusChanged;

    /// <summary>Whether it plays (wanted, ready, prepared) changed.</summary>
    public event Action<bool>? PlayingChanged;

    /// <summary>The first picture moves: the screen's cover can go.</summary>
    public event Action? FirstMove;

    /// <summary>A seek played on from where it landed, <paramref name="landed"/> on the film's timeline.</summary>
    public event Action<long>? Settled;

    public EngineStatus Status { get; private set; } = EngineStatus.Buffering;

    public bool Playing { get; private set; }

    /// <summary>Whether the first picture has moved.</summary>
    public bool Moved => moved;

    /// <summary>A seek on its way: the player's reads aren't taken until it is done.</summary>
    public long? Pending => pending;

    /// <summary>Counts seeks asked and done: a read asked at an earlier count is dropped.</summary>
    public long Epoch { get; private set; }

    /// <summary>Where the trimmed playlists begin on the film's timeline; -1 when not trimmed.</summary>
    public long WindowStart { get; private set; } = -1;

    /// <summary>How often the engine should read the position now.</summary>
    public uint ReadEveryMs => !moved ? StartEveryMs : settling ? SettleEveryMs : EveryMs;

    /// <summary>Where it plays on the film's timeline: the seek on its way, else the last read, run on while it plays.</summary>
    public long PositionMs
    {
        get
        {
            if (pending is long seek) return seek;
            if (Playing && moved && readAt > 0) return last + Math.Min(now() - readAt, ProjectMs);
            return last;
        }
    }

    /// <summary>The player's own position for <paramref name="ms"/> on the film's timeline.</summary>
    public long ToPlayer(long ms) => Math.Max(0, ms - shift);

    /// <summary>Whether <paramref name="ms"/> is before the trimmed playlists begin: they don't have it.</summary>
    public bool Before(long ms) => WindowStart >= 0 && ms < WindowStart;

    /// <summary>
    /// Prepared from playlists trimmed to begin at <paramref name="startMs"/>: the player stands
    /// there, and is taken to count from there until its first read says otherwise.
    /// </summary>
    public void Trimmed(long startMs)
    {
        WindowStart = startMs;
        shift = startMs;
        shiftKnown = false;
        Jumped(startMs);
    }

    /// <summary>The player was sent to <paramref name="ms"/> before it played (where a film resumes): taken to be there.</summary>
    public void Jumped(long ms)
    {
        last = ms;
        readAt = 0;
        Epoch++;
    }

    /// <summary>The player is prepared; it starts if it is to play, and waits for its first picture.</summary>
    public void Opened()
    {
        opened = true;
        Update();
    }

    public void Play()
    {
        wanted = true;
        Update();
    }

    public void Pause()
    {
        wanted = false;
        waiting = false;
        Update();
    }

    /// <summary>A seek to <paramref name="ms"/> is on its way.</summary>
    public void Seeking(long ms)
    {
        pending = ms;
        Epoch++;
    }

    /// <summary>
    /// The seek to <paramref name="ms"/> is done (or refused): it waits until it plays on from where
    /// it landed, or plays again from the end. False when another seek came after it.
    /// </summary>
    public bool Sought(long ms)
    {
        if (pending != ms) return false;
        pending = null;
        Jumped(ms);
        waiting = false;
        // Back from the end too: it plays again.
        ended = false;
        if (moved)
        {
            settling = true;
            settleFrom = -1;
        }
        Update();
        return true;
    }

    /// <summary>The player says it played to the end.</summary>
    public void Ended()
    {
        ended = true;
        Update();
    }

    /// <summary>
    /// A read of the player's position, asked at <paramref name="epoch"/>: <paramref name="raw"/> its
    /// own position (negative when it wasn't playing or paused), <paramref name="playing"/> whether
    /// it was playing. Weighed only by a read that asked whether it plays (<paramref name="weigh"/>):
    /// a read for the clock alone moves the position on.
    /// </summary>
    public void Read(long epoch, long raw, bool playing, bool weigh = true)
    {
        if (epoch != Epoch || pending != null || !opened) return;
        if (raw >= 0 && (playing || !weigh))
        {
            if (playing) Learn(raw);
            last = raw + shift;
            readAt = now();
        }
        if (weigh) Weigh(last, playing);
    }

    /// <summary>A trimmed playlist's player's first position while it plays: near 0, it counts from the playlist; else from the film.</summary>
    void Learn(long raw)
    {
        if (WindowStart < 0 || shiftKnown) return;
        shiftKnown = true;
        shift = raw < WindowStart / 2 ? WindowStart : 0;
    }

    /// <summary>Whether the player counts from its trimmed playlist (true) or from the film (false); null until its first read says.</summary>
    public bool? CountsFromPlaylist => WindowStart < 0 || !shiftKnown ? null : shift > 0;

    void Weigh(long position, bool playing)
    {
        if (ended) return;
        if (!moved)
        {
            if (!playing) return;
            if (firstAt < 0) firstAt = position;
            if (position < firstAt + FirstMoveMs) return;
            moved = true;
            Still(position);
            FirstMove?.Invoke();
            Update();
            return;
        }
        if (settling)
        {
            if (wanted && !playing) return;
            if (wanted && settleFrom < 0)
            {
                settleFrom = position;
                return;
            }
            if (wanted && position < settleFrom + FirstMoveMs) return;
            // Playing on, or paused: nothing to wait for.
            settling = false;
            Still(position);
            Settled?.Invoke(settleFrom < 0 ? position : settleFrom);
            Update();
            return;
        }
        if (!wanted || !playing || position != watchedAt)
        {
            Still(position);
            waiting = false;
            Update();
            return;
        }
        if (now() - stillSince < StillMs) return;
        waiting = true;
        Update();
    }

    void Still(long position)
    {
        watchedAt = position;
        stillSince = now();
    }

    void Update()
    {
        var status = ended ? EngineStatus.Ended
            : !opened ? EngineStatus.Buffering
            : wanted && (!moved || settling || waiting) ? EngineStatus.Buffering
            : EngineStatus.Ready;
        var playing = wanted && opened && status == EngineStatus.Ready;
        if (status != Status)
        {
            Status = status;
            StatusChanged?.Invoke(status);
        }
        if (playing != Playing)
        {
            Playing = playing;
            PlayingChanged?.Invoke(playing);
        }
    }
}
