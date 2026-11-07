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
  Plex.
- **Restrictions.** The profile of a Plex Home user Plex limits (a managed
  user, or a member with their own Plex account and restrictions on what
  you share with them) gets an age limit from the ratings Plex allows them,
  or below the youngest it disallows, and label rules from Plex's labels:
  titles with a label they're always allowed are shown above their age
  limit, titles with a label they're never allowed aren't shown at all. A
  rule Pley can't keep, such as "only titles with this label", becomes the
  strictest it can, and the import says which. So does a list of
  disallowed ratings with a gap (PG-13 disallowed but R allowed): an age
  limit hides everything above it. A managed user's Plex preset becomes an
  age limit alone: 5 for Little kid, 11 for Older kid and 14 for Teen.
- **Titles.** Plex's labels are added to the titles, and a title's date
  added moves to Plex's when that's earlier.
- **What everyone watched.** Watched titles, resume points, ratings and
  each play, with the device it was on.

Collections, playlists, edited metadata and artwork don't come over yet.

## How titles are found

Pley scans the folders itself, then finds each Plex title among its own:
first by its file (the same name and size, under the same folder), then by
its TMDB, IMDb or TheTVDB id, an episode by its show's id and its numbers.
Never by its title alone. Music is found by its file only.

When Pley sees Plex's folders under another path, say the server runs in
Docker with `/data/movies` mounted as `/media/movies`, say where each one
is on the Libraries step. Pley works out most of them from the files it
finds.

## Importing

On a new server, after the administrator account is made, Pley asks
"Coming from Plex?". Later, it's under Settings › Libraries › Import from
Plex.

1. **Sign in with Plex.** Plex's sign-in page opens in a window; allow Pley
   there. A Plex token works too, for a server you sign in to another way.
2. **Choose the server.** Only a server your account owns can be imported:
   only its owner can read everyone's history and the Home's restrictions.
   A server Plex doesn't list, or that Pley can't reach through Plex, is
   chosen by its address.
3. **Libraries.** Each Plex library shows where it goes: *In Pley* (its
   files are in a Pley library), *Check this* (some are, or in more than
   one) or *Not in Pley* (a new library is made for it from Plex's folders,
   or it's left out when Pley can't open them until you say where they
   are). Change where any of them goes, or leave it out.
4. **People.** Each Home user shows the profile they'll get and its limits.
   Turn off anyone who shouldn't come. Plex sometimes asks for a protected
   user's PIN; type it there, or they're skipped.
5. **Import.** New libraries are scanned first, then everything else comes
   over. You can close the page; the import goes on, and the report is
   there when you come back.

The report says how many titles each library found, the ones it didn't,
and what came over for each person.

## Importing again

Running the import again brings what's new and nothing twice. It never
takes anything away: labels are only added, a date added only moves
earlier, a title watched in Pley stays watched, and a resume point from
after Plex's stays. Profiles made by an earlier import are found again;
they keep the age limit set in Pley (or get Plex's, when they have none)
and get Plex's new labels added to their rules.

Stopping an import keeps what came over, and makes Pley forget the Plex
sign-in. Pley keeps the sign-in in memory only, so restarting the server
forgets it too.
