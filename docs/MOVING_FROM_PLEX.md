# Moving from Plex

Pley brings over what a Plex Media Server knows about your libraries and
the people who watch them, so nobody starts again from nothing. It reads the
server through Plex's own API while it runs, never its database, and it
never changes anything on Plex.

## What comes over

- **Libraries.** Each Plex movie, show or music library goes to a Pley
  library: one that already has its files, or a new one made from its
  folders. Photo libraries stay behind.
- **People.** The Plex account you sign in with comes as you, the
  administrator, with its Plex name as your display name if you haven't
  set one. Each person in your Plex Home you choose becomes a
  profile of Pley's household, opening the libraries they could open on
  Plex. Plex's picture for each comes too, where the profile has none in
  Pley: one chosen on Plex or a Gravatar. A Plex picture that's only an
  initial brings its colour instead, for Pley's own initial.
- **Restrictions.** The profile of a Plex Home user with restrictions (a
  managed user's, or those on what you share with a member who has their
  own Plex account) gets an age limit from the ratings Plex allows them,
  or below the youngest it disallows, and label rules from Plex's labels:
  titles with a label they're always allowed are shown above their age
  limit, titles with a label they're never allowed aren't shown at all. A
  rule Pley can't keep becomes the strictest it can, and the import says
  which: "only titles with this label", a field other than ratings and
  labels, a disallowed rating Pley doesn't know, or a label never allowed
  that can't be kept (past 50 of them, or longer than a label can be)
  give the youngest age limit. A list of disallowed ratings with a gap
  (PG-13 disallowed but R allowed) hides everything above the youngest of
  them: an age limit can't have gaps. Disallowing unrated titles is what
  any age limit does, so on its own it gives the oldest. A managed user's Plex preset becomes an
  age limit alone: 5 for Little kid, 11 for Older kid and 14 for Teen. A
  managed user without restrictions sees everything, as on Plex. A
  profile with labels it's never allowed is off until the import
  finishes, since Plex's labels come to Pley's titles later in it; you can
  turn it on sooner in Settings › Users. Titles Pley couldn't find on Plex
  don't get Plex's labels, so those rules don't hide them.
- **Titles.** Plex's labels are added to the titles, and a title's date
  added moves to Plex's when that's earlier.
- **What everyone watched.** Watched titles, resume points, ratings and
  each play, with the device it was on.
- **Collections and playlists.** Collections, in their order and with
  their labels, and each person's playlists, as theirs. Smart ones stay
  behind: Plex's filters don't turn into Pley's.
- **Matches and edits.** A title Plex matched differently from Pley is
  matched to Plex's choice, as Fix Match does (Pley needs its TMDB key
  for that). Fields edited in Plex, which Plex locks, come over as edits
  in Pley: titles, summaries, years, content ratings, genres and the
  like.

Titles found by their files show with Plex's names, years, summaries
and dates added as soon as their library is scanned, while the other
libraries still are. Pley's own details replace the names, years and
summaries when it fetches them; titles keep Plex's where Pley's search
finds nothing. Posters and other artwork don't come over: they come from
Pley's fetch.

The last three are choices on the import's "What to bring" step, all on
at first. If you use Kometa, some of what it made in Plex comes over this
way: it locks the fields it sets, such as content ratings, and its
collections are collections like any. Point Kometa at Pley afterwards
(see [Compatibility](COMPATIBILITY.md)) and it carries on there, its
overlays included: its Overlay label stays behind, so it draws them again
on Pley's posters.

## How titles are found

Pley scans the folders itself, then finds each Plex title among its own:
first by its file (the same name and size, under the same folder), then by
its TMDB, IMDb or TheTVDB id, an episode by its show's id and its numbers.
Never by its title alone. Music is found by its file only.

What's found by its file comes over before Pley fetches any posters or
details, so what everyone watched is there within minutes, even for a
large library. With matches and edits brought, Plex's matches go on the
titles first, and Pley fetches each one by Plex's ids rather than
searching by its name. Titles found only by their ids, which the fetch
gives Pley, come over once it's done.

When Pley sees Plex's folders under another path, say Plex runs in Docker
with your media at `/data/media` and Pley with it at `/media`, Pley looks
for them: under `/`, `/mnt`, `/media`, `/srv`, `/Volumes`, your home, the
folders mounted into its container and the folders directly inside these,
by the end of Plex's path (`media/movies`, then `movies`), and takes a
folder only when one of Plex's files is there. Any it doesn't find, say on
the Libraries step: one folder given says where the others are too
(`/data/media/movies` at `/media/movies` puts `/data/media/tv` at
`/media/tv`), and Pley fills in those it finds there.

## Importing

On a new server, after the administrator account is made and the server's
name, language and country are set, Pley asks how you'd like to start:
choose Bring over my Plex server. Later, it's under Settings › Libraries ›
Import from Plex.

1. **Sign in with Plex.** Plex's sign-in page opens in a window; allow Pley
   there. A Plex token works too, for a server you sign in to another way.
2. **Choose the server.** Only a server your account owns can be imported:
   only its owner can read everyone's history and the Home's restrictions.
   A server Pley can't reach at the addresses Plex gives is chosen by its
   address.
3. **Libraries.** Each Plex library shows how much it holds and where it
   goes, which follows its files: *In Pley* (its files are in a Pley
   library, and it goes there), *Check this* (only some are, or they're in
   more than one; it goes into the one with most, and what Plex knows of
   the others doesn't come) or *Not in Pley* (a new library is made for it
   from Plex's folders, under Plex's name, or it's left out when Pley can't
   find them, with each folder asked for: "Plex sees `/data/media/tv`.
   Where does Pley see it?"; giving one Pley can open brings it). Plex's
   libraries stay apart: none goes into another Pley library. Under each
   folder, Pley says whether Plex's files are there, checking a few of them
   by name and size, as the folder changes. Turn off any library that
   shouldn't come. Rename a new library afterwards, in Settings ›
   Libraries. On a server with no libraries yet, the badges aren't shown:
   every library would be Not in Pley.
4. **People.** Each Home user shows the profile they'll get and its limits.
   Turn off anyone who shouldn't come. Plex sometimes asks for a protected
   user's PIN; type it there, or they're skipped.
5. **What to bring.** Watch history, collections and playlists, matches
   and edits: turn off what you'd rather start over in Pley.
6. **Import.** Once started, Pley goes to Home (or back to Settings ›
   Libraries, from there), which shows the import: which step it's on, how
   far the whole import has come, and its steps, ticked off with what each
   brought, with **Stop the import**. New libraries are scanned for their
   files, a library at a time: each shows its files read and about how
   long that has left, then the titles found in it. What's found by file
   comes over with Plex's names, then Pley fetches posters and details and
   looks for the rest of Plex's titles. **Hide the steps** keeps it to a
   line for that import. The browser tab's title says how far it is, on
   any page, and how it ended if that was while the tab was out of sight.
   You can watch, or close the page, at any time; the import goes on. Once it
   ends, Home says how it went, why if it failed, and has **See what came
   over**, its report, until you close it; Settings › Libraries keeps
   saying when it ran. An import that failed or was stopped, or that a
   restart of the server cut short, has **Import again**, which picks up
   where it left off: what came over stays (see below).

The report says how many titles each library found, the ones it didn't,
what came over for each person, and how long the import took, opening on
how long each step did.

## Importing again

Running the import again brings what's new and nothing twice. It never
takes anything away: labels are only added, a date added only moves
earlier, a title watched in Pley stays watched, and a resume point from
after Plex's stays. Edits made in Pley stay, and so does a title matched
with Fix Match in Pley, locked, or matched by its NFO file. A collection or playlist found again by its title
gets the titles it lacks, and nothing is reordered or removed. Profiles
made by an earlier import are found again;
they keep the age limit set in Pley (or get Plex's, when they have none),
and their picture and colour, and get Plex's new labels added to their
rules. A library the import makes opens for those who could open it on
Plex; one already in Pley stays as you set it.

Stopping an import keeps what came over, and the libraries it made fetch
their posters and details all the same, as they do after a restart cuts
one short. However an import ends, Pley
forgets the Plex sign-in, so importing again starts by signing in. Pley
keeps the sign-in in memory only, so restarting the server forgets it
too.
