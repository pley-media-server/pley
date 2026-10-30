# How to name your media

Keep movies, shows and music in separate folders, name them as below, and
add one library per folder in **Settings → Libraries**. Pley never changes
or moves your files.

This folder is mounted read-only at `/media` in the container, so the
libraries are `/media/Movies`, `/media/TV Shows` and `/media/Music`. To use
media you already have, set `PLEY_MEDIA_DIR` in `.env` to its folder
instead. Git ignores what you add here.

## Movies

```
Movies/
├── Arrival (2016).mkv
├── Arrival (2016)-trailer.mkv                      an extra
├── Dune Part Two (2024) {tmdb-693134}.mkv          a TMDB id fixes the match
└── Blade Runner (1982)/                            a folder works too
    ├── Blade Runner (1982) {edition-Final Cut}.mkv
    ├── Blade Runner (1982) {edition-Final Cut}.en.srt
    ├── poster.jpg
    └── Trailers/
        └── Official Trailer.mkv
```

- `Title (Year)`, as a file or a folder. The year makes the match certain.
- Release names (`Title.2019.2160p.WEB-DL.mkv`) work too.
- `{edition-...}` marks an edition of the same movie.
- Extras go in a folder such as `Trailers`, `Featurettes` or
  `Behind The Scenes` next to the movie.

## TV shows

```
TV Shows/
└── Severance (2022)/
    ├── poster.jpg
    ├── Season 01/
    │   ├── Severance (2022) - S01E01 - Good News About Hell.mkv
    │   └── Severance (2022) - S01E02 - Half Loop.mkv
    ├── Season 02/
    │   └── Severance - 2x01 - Hello, Ms. Cobel.mkv
    └── Specials/                                   season 0
        └── Severance - S00E01.mkv
```

- One folder per show, `Season NN` folders, and `S01E02` (or `1x02`) in
  each file name. Inside a season folder, `02 - Title` is enough.
- Date-based shows (`Show - 2024-03-01.mkv`) and anime numbering
  (`Show - 01.mkv`) work too.

## Music

```
Music/
└── Daft Punk/
    └── Discovery (2001)/
        ├── cover.jpg
        ├── 01 - One More Time.flac
        └── 02 - Aerodynamic.flac
```

`Artist/Album/track`, with the tags in the files. Covers come from
`cover.jpg` or `folder.jpg`, or from the files.

## Subtitles and artwork

- **Subtitles** sit next to the video with its name, a language, and
  optionally `forced` or `sdh`: `Movie (2019).en.srt`,
  `Movie (2019).eng.forced.srt`. A `Subs` folder next to it works too.
- **Artwork** next to the media wins over downloaded artwork: `poster.jpg`,
  `fanart.jpg`, `season01-poster.jpg`, `Movie (2019)-fanart.jpg`.
