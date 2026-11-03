//! How capable the TV is.

/// How much the TV can take. Memory and GPU budgets, motion and prefetch
/// distances follow it (the Android app's device tiers; the LG app chooses one
/// from the TV's facts: Low for webOS 4 and older or under 1.5 GB of memory,
/// High from webOS 22).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    Low,
    #[default]
    Mid,
    High,
}

impl Tier {
    /// The name the capability report and Diagnostics use.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Low => "low",
            Tier::Mid => "mid",
            Tier::High => "high",
        }
    }
}
