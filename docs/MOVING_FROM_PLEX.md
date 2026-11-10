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
  administrator. Each person in your Plex Home you choose becomes a
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
  rule Pley can't keep, such as "only titles with this label", becomes the
  strictest it can, and the import says which. So does a list of
  disallowed ratings with a gap (PG-13 disallowed but R allowed): an age
  limit hides everything above it. A managed user's Plex preset becomes an
  age limit alone: 5 for Little kid, 11 for Older kid and 14 for Teen. A
  managed user without restrictions sees everything, as on Plex.
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
3. **Libraries.** Each Plex library shows where it goes: *In Pley* (its
   files are in a Pley library), *Check this* (some are, or in more than
   one) or *Not in Pley* (a new library is made for it from Plex's folders,
   or it's left out when Pley can't find them, with each folder asked for:
   "Plex sees `/data/media/tv`. Where does Pley see it?"; giving one brings
   it). Folders Pley found say so, to check. Change where any library
   goes, or leave it out. On a server with no libraries yet, the badges
   aren't shown: every library would be Not in Pley.
4. **People.** Each Home user shows the profile they'll get and its limits.
   Turn off anyone who shouldn't come. Plex sometimes asks for a protected
   user's PIN; type it there, or they're skipped.
5. **What to bring.** Watch history, collections and playlists, matches
   and edits: turn off what you'd rather start over in Pley.
6. **Import.** It lists every step it goes through, ticking each off
   with what it brought. New libraries are scanned for their files, a
   library at a time (it shows which, and its files read), what's found by file comes
   over with Plex's names, then Pley fetches posters and
   details and looks for the rest of Plex's titles. Once what everyone watched is in, **Start
   watching** (or **Close**, from Settings) lets you go on while the rest
   runs. You can close the page at any time; the import goes on.
   Home shows which step it is on and how far the whole import has come
   while it runs, and so does Settings › Libraries, which once it's done
   says when it ran and has **See what came over**, its report.

The report says how many titles each library found, the ones it didn't,
and what came over for each person.

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
rules.

Stopping an import keeps what came over, and makes Pley forget the Plex
sign-in. Pley keeps the sign-in in memory only, so restarting the server
forgets it too.
