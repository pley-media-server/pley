//! The icons the app draws: Lucide's (ISC licence), on a 24-unit grid, as the
//! Android TV app and the web TV app draw them. `pley-gfx` holds their paths
//! and rasterises them at device pixels.

/// An icon by its Lucide name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    ArrowDownWideNarrow,
    ArrowLeft,
    ArrowRight,
    ArrowUpDown,
    ArrowUpNarrowWide,
    AudioLines,
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
    Delete,
    Disc3,
    Ellipsis,
    EyeOff,
    FastForward,
    Film,
    Funnel,
    Gauge,
    GripVertical,
    History,
    House,
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
    Moon,
    Music,
    Pause,
    Pencil,
    Play,
    Plus,
    Radio,
    RefreshCw,
    Repeat,
    Repeat1,
    Rewind,
    RotateCcw,
    RotateCw,
    Save,
    Search,
    Server,
    ServerOff,
    Settings,
    Shuffle,
    SkipBack,
    SkipForward,
    SlidersHorizontal,
    Star,
    /// The star's outline alone (an empty rating star).
    StarOutline,
    Tag,
    Timer,
    Trash2,
    Tv,
    TvMinimal,
    UserRound,
    Users,
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
