//! A category's channels as the page shows them: the pages the server answered, or before the
//! first of them comes, the first page saved when the category last showed (`live:page:<key>:0:`),
//! so the guide draws at once.

use pley_core::model::{LiveChannel, LiveChannelPage};
use pley_player::live::ChannelPages;

/// The channels of the category listed.
#[derive(Clone, Copy)]
pub struct Lineup<'a> {
    pub pages: &'a ChannelPages,
    /// The first page saved, shown until the server answers.
    pub saved: Option<&'a LiveChannelPage>,
}

impl<'a> Lineup<'a> {
    /// How many channels the category has; none until known.
    pub fn total(&self) -> Option<i64> {
        self.pages.total().or(self.saved.map(|s| s.total))
    }

    /// The channel at `index`, once its page is in.
    pub fn at(&self, index: i64) -> Option<&'a LiveChannel> {
        if self.pages.total().is_some() {
            return self.pages.at(index);
        }
        let saved = self.saved?;
        saved.channels.get(usize::try_from(index - saved.offset).ok()?)
    }
}

/// The first page of `pages` once it is all in, to save for next time.
pub fn first_page(pages: &ChannelPages) -> Option<LiveChannelPage> {
    let total = pages.total()?;
    let n = total.min(pley_player::live::data::CHANNEL_PAGE);
    let channels = (0..n).map(|i| pages.at(i).cloned()).collect::<Option<Vec<_>>>()?;
    Some(LiveChannelPage { total, offset: 0, channels })
}
