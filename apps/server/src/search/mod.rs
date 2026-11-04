//! Search: the titles, people and Live TV channels a query finds, and in
//! what order. Everything else asks this module ([`items`], [`people`],
//! [`Names`]) and knows nothing of how it finds them.
//!
//! A query's words each begin a word of a name, in any order; a year at the
//! end of a title's search narrows it to that year's titles. A word no name
//! has is read as the closest words names have: a slip ("matirx"), words
//! joined or split ("spiderman"), an apostrophe left out ("captains"), the
//! query's other words choosing between them ([`fuzzy`]). When that finds
//! nothing, names alike enough to the whole query are found by their
//! pieces of three letters ([`trigram`]).
//!
//! Titles and people are matched by the database's full-text indexes
//! (`items_fts`, `people_fts`); the words of those indexes and every title
//! and name, folded, are kept in memory ([`Index`]), read again after the
//! library changes, and their pieces are worked out when a search first
//! needs them. Search writes nothing to the database: its backups
//! carry no search data, and another way of searching can take this one's
//! place by replacing this module.

pub mod fuzzy;
mod library;
pub mod names;
pub mod text;
pub mod trigram;

pub use library::{Index, items, people};
pub use names::{Found, Names};
pub use text::match_rank;
