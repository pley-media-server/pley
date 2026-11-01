# Backports: native Samsung app to the other apps

Features and rule fixes that started in the native Samsung app (`apps/tizen-native`) and are to
reach the other apps, as a feature people see works the same everywhere (AGENTS.md). Each says what,
the native commit, and the apps it still has to reach; it is ticked once it has reached them all.

Apps: **AndTV** Android TV · **AndPh** Android phone/tablet · **Web** web + desktop · **TV** Samsung
and LG web apps (`packages/tv`) · **Roku** · **Kodi**.

## Player

- [ ] **OK shows the controls instead of pausing.** With the controls hidden, OK shows them on
  the scrubber (as Up does); OK on the scrubber plays/pauses; OK just after a seek confirms it.
  Native `d80bdfb1`. To: AndTV, TV, Roku, Kodi (remote-driven players).
- [ ] **Chapters as a row of cards (YouTube-style).** While a seek waits for OK (Right or Left
  on the scrubber), Up opens a row of chapter cards (picture: the chapter's own, else the
  trickplay frame a moment in; name; start time), from where the seek was going, and hides Skip
  Intro. Pauses while open, plays on as it was when it closes; Left/Right move the scrubber's
  thumb and times to each chapter's start; OK jumps there; Down/Back close. Up with no seek
  waiting does what it did (Skip Intro while it shows). Native `d80bdfb1` and "seeking that waits for OK"
  (`Screens/ChapterStrip.cs`). To: AndTV, TV, Roku, Kodi; Web/AndPh could get a similar picker.
- [ ] **A seek waits for OK; a held key speeds up smoothly (YouTube's TV app).** Left/Right
  move the target with its preview while the video plays on; OK goes there, Back lets it go, and
  it goes by itself 5 s after the last key (media keys' jumps still go after 0.4 s). Held, it
  scrubs at 20× and twice as fast every 1.5 s, up to a whole video in 10 s (300× at least);
  letting go waits for OK too. Native "seeking that waits for OK" (`Seeker`, tested). To: AndTV, TV, Roku, Kodi.
- [ ] **Timestamp chapter names are numbered.** A chapter titled only by where it starts
  ("00:06:19.212", mkvmerge's default) shows as "Chapter N" in the seek preview, the Chapters
  panel and the cards; a real name containing a time keeps it. Native `d80bdfb1`
  (`PlayerText.NameOf`, tested). To: every app that shows chapter names (shared rule; a
  `packages/tv` fixture would pin it).
- [ ] **Scrubber fill follows the thumb.** The blue part runs to the thumb while seeking or on a
  chapter card, not only to where it plays. Native `d80bdfb1`. Check what Android TV does first.
- [ ] **Title banner as each item begins.** When an item starts playing (the next episode by
  itself included) with the controls hidden, the top of the controls (title and episode line over
  the top scrim) shows for 4 s, and makes way if the controls open. Native `a1eb3df6`
  (`PlayerScreen.Banner`). Check what Android TV shows on autoplay. To: AndTV, TV, Roku, Kodi.

## Live TV

- [ ] **stream.ts for sound the device decodes.** The plan's `direct` asks for video and audio
  as they are; the native app also plays `stream.ts` when the audio is converted only for players
  that can't decode it (AC3, E-AC3, MP2 on every Samsung TV), where HLS took ~0.8 s longer and
  cost the server a conversion. Native "Live TV starts from the channel's stream.ts"
  (`LiveRules.PlaysAsSent`, tested). AndTV: the same, by what the device's decoders take.
- [ ] **Left through empty guide time.** On a channel with no programmes, Left goes back a half
  hour at a time as Right goes on; before, it left the guide for the categories from wherever
  Right had gone. Native `925375c7` (`GuideCursor.MoveLeft`, test changed). The rule came from
  Android TV's guide: AndTV, TV.
- [ ] **Leaving the guide takes a press of its own.** Left at the guide's start goes to the
  categories only on a fresh press (not under a held key, nor within 0.5 s of a Left that moved
  in the guide); a held Up at the top of the categories or the guide stops there (the tabs take a
  press). Native `925375c7`, `003be8ae`. Check Android TV's behaviour before porting.
- [ ] **Back leaves the guide on a channel without programmes.** `GuideCursor.AwayFromNow` took
  no programme at the cursor as away from now always, so Back never left the guide there. Native
  `18a6d523` (tested). Came from Android TV's rule: AndTV, TV.
- [ ] **Each press moves the guide on a channel without programmes.** Right moved the cursor
  unseen until it neared the edge (two presses), Left moved the view at once. With nothing in view,
  each press moves the view a half hour, both ways. Native `323230c3` (tested). AndTV, TV.
- [ ] **Left goes back through a long programme before leaving the guide.** On a programme that
  began with the guide, with the view scrolled on, Left left for the categories at once. Now the view
  goes back a screen at a time first. Native `a6829f7d` (tested). AndTV, TV.

## Navigation

- [ ] **A held key stays in a library grid.** Held Right at a row's end doesn't reach the A–Z
  jump, held Up at the top doesn't reach the chips/tabs; a fresh press does. Native `d54973b4`.
  Check Android TV.
