//! The simulator's command line (DESIGN §6). Every option is optional; the TV starts the binary
//! with none (webOS passes its launch parameters as one JSON argument, which is ignored here).

use std::path::PathBuf;

use pley_base::Tier;

/// What the command line asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// `--server <url>`: use this server.
    pub server: Option<String>,
    /// `--fixtures`: start the fake server and sign in to it as its household, with the device code.
    pub fixtures: bool,
    /// `--data <dir>`: where the app keeps its data (a folder under the temporary folder by default).
    pub data: Option<PathBuf>,
    /// `--size 1920x1080`: the window's size in pixels (1280×720 by default).
    pub size: Option<(u32, u32)>,
    /// `--keys "<script>"`: keys and waits to play once the first page shows.
    pub keys: Option<String>,
    /// `--screenshot <file.png>`: after the script, write the frame there and leave.
    pub screenshot: Option<PathBuf>,
    /// `--route <route>`: open a page directly (`home`, `item:12`, `settings:about`…).
    pub route: Option<String>,
    /// `--reduced-motion`: as Settings' Reduce motion.
    pub reduced_motion: bool,
    /// `--tier low|mid|high`: act as a TV of that tier.
    pub tier: Option<Tier>,
    /// `--stats`: print the time to the first frame and the frame times on leaving.
    pub stats: bool,
    /// `--verbose`: log debug lines too.
    pub verbose: bool,
}

/// What `--help` prints.
pub const USAGE: &str = "\
pley: Pley for LG TV (the simulator on a computer)

  --server <url>         use this Pley server
  --fixtures             start the fake server and sign in to it
  --data <dir>           keep the app's data here
  --size <w>x<h>         the window's size (1280x720)
  --keys \"<script>\"      keys to press once the first page shows: up down left right ok back menu
                         info play pause playpause stop ff rew next prev chup chdown red green
                         yellow blue 0-9, wait:<ms>, text:<chars>, point:<x>,<y> (dp), click
  --screenshot <file>    after the keys, write the frame as PNG and leave
  --route <route>        open a page: home, item:12, library:3, search:<query>, settings:about…
  --reduced-motion       as Settings' Reduce motion
  --tier low|mid|high    act as a TV of that tier
  --stats                print the time to the first frame and the frame times on leaving
  --verbose              log debug lines too

Keys: arrows, Enter (OK), Escape or Backspace (Back), M or F1 (menu), I (info), F2-F5 (colour
keys), Space (play/pause), , and . (rewind, fast forward), Page Up/Down (CH+/CH-), digits; the
mouse is the Magic Remote.";

/// Reads the command line (without the program's name). `Ok(None)`: `--help` was asked.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--server" => out.server = Some(value("--server")?),
            "--fixtures" => out.fixtures = true,
            "--data" => out.data = Some(PathBuf::from(value("--data")?)),
            "--size" => out.size = Some(size(&value("--size")?)?),
            "--keys" => out.keys = Some(value("--keys")?),
            "--screenshot" => out.screenshot = Some(PathBuf::from(value("--screenshot")?)),
            "--route" => out.route = Some(value("--route")?),
            "--reduced-motion" => out.reduced_motion = true,
            "--tier" => out.tier = Some(tier(&value("--tier")?)?),
            "--stats" => out.stats = true,
            "--verbose" | "-v" => out.verbose = true,
            // webOS's launch parameters: a JSON object as the only argument.
            s if s.starts_with('{') => {}
            other => return Err(format!("unknown option {other} (try --help)")),
        }
    }
    Ok(Some(out))
}

fn size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once(['x', 'X']).ok_or_else(|| format!("--size wants <w>x<h>, not {s}"))?;
    let n = |v: &str| v.trim().parse::<u32>().ok().filter(|n| (64..=7680).contains(n));
    match (n(w), n(h)) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(format!("--size wants <w>x<h>, not {s}")),
    }
}

fn tier(s: &str) -> Result<Tier, String> {
    match s {
        "low" => Ok(Tier::Low),
        "mid" => Ok(Tier::Mid),
        "high" => Ok(Tier::High),
        _ => Err(format!("--tier wants low, mid or high, not {s}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(s: &[&str]) -> Result<Option<Args>, String> {
        parse(s.iter().map(|s| s.to_string()))
    }

    #[test]
    fn options_are_read() {
        let a = parse_all(&[
            "--fixtures",
            "--size",
            "1920x1080",
            "--keys",
            "down ok",
            "--screenshot",
            "shot.png",
            "--tier",
            "low",
            "--route",
            "item:12",
            "--reduced-motion",
            "--stats",
        ])
        .unwrap()
        .unwrap();
        assert!(a.fixtures && a.reduced_motion && a.stats);
        assert_eq!(a.size, Some((1920, 1080)));
        assert_eq!(a.keys.as_deref(), Some("down ok"));
        assert_eq!(a.screenshot, Some(PathBuf::from("shot.png")));
        assert_eq!(a.tier, Some(Tier::Low));
        assert_eq!(a.route.as_deref(), Some("item:12"));
    }

    #[test]
    fn mistakes_are_said() {
        assert!(parse_all(&["--size", "big"]).is_err());
        assert!(parse_all(&["--tier", "ultra"]).is_err());
        assert!(parse_all(&["--server"]).is_err());
        assert!(parse_all(&["--nope"]).is_err());
        assert_eq!(parse_all(&["--help"]), Ok(None));
        // webOS's launch parameters are not options.
        assert_eq!(parse_all(&["{\"x\":1}"]), Ok(Some(Args::default())));
    }
}
