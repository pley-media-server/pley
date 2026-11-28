//! The icons the app draws: Lucide's (ISC license), on a 24-unit grid, as the
//! Android TV app and the web TV app draw them, and Pley's mark. `pley-gfx`
//! holds their paths and rasterises them at device pixels.

use crate::mark::MarkGlyph;

/// An icon by its Lucide name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Activity,
    AppWindow,
    ArrowDownWideNarrow,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowUpDown,
    ArrowUpNarrowWide,
    AudioLines,
    Ban,
    Bell,
    BellOff,
    BookmarkCheck,
    BookmarkMinus,
    BookmarkPlus,
    Calendar,
    Captions,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronsRight,
    CircleAlert,
    CircleCheck,
    CircleX,
    Clapperboard,
    Clock,
    Compass,
    CornerLeftUp,
    Delete,
    Disc3,
    Droplet,
    Ellipsis,
    EyeOff,
    FastForward,
    FileVideo,
    Film,
    Folder,
    FolderOpen,
    Funnel,
    Gauge,
    Globe,
    GripVertical,
    History,
    House,
    Inbox,
    Info,
    Languages,
    Layers,
    List,
    ListMusic,
    ListOrdered,
    ListPlus,
    ListVideo,
    Lock,
    LogOut,
    Monitor,
    Music,
    Palette,
    Pause,
    Pencil,
    Play,
    /// Pley's mark (not Lucide's, [`crate::mark`]): its three posters, back to
    /// front, at 16, 24 and 36 % of the tint, the glyph cut out of the front
    /// one, as a placeholder without a picture shows it.
    PleyMark(MarkGlyph),
    Plus,
    Radio,
    RefreshCw,
    Repeat,
    Repeat1,
    Rewind,
    RotateCcw,
    Save,
    Search,
    Server,
    ServerOff,
    Settings,
    Shuffle,
    SkipBack,
    SkipForward,
    SlidersHorizontal,
    Sparkles,
    Square,
    Star,
    /// The star's outline alone (an empty rating star).
    StarOutline,
    Tag,
    Timer,
    Trash2,
    Tv,
    TvMinimal,
    Usb,
    UserRound,
    Users,
    Volume2,
    WifiOff,
    X,
}

impl Icon {
    /// Icons drawn filled as well as stroked, as both TV apps draw them.
    pub fn filled(self) -> bool {
        matches!(self, Icon::Play | Icon::Pause | Icon::SkipBack | Icon::SkipForward | Icon::Star)
    }

    /// The stroke width, in grid units, for an icon drawn `size` dp square:
    /// thinner as icons grow, so they look alike at every size.
    pub fn stroke_for(size: f32) -> f32 {
        (2.0 + (24.0 - size) / 16.0).clamp(1.5, 2.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_follows_size() {
        assert_eq!(Icon::stroke_for(24.0), 2.0);
        assert_eq!(Icon::stroke_for(20.0), 2.25);
        assert_eq!(Icon::stroke_for(48.0), 1.5);
        assert_eq!(Icon::stroke_for(9.0), 2.5);
    }
}
