//! What a page is to the shell ([`Screen`]), what the shell tells it ([`AppEvent`]), how it
//! shows ([`Chrome`]), and the [`Registry`] the app fills with every page's screen.

use pley_base::{Point, Rect, TextOpts};
use pley_core::DeviceEvent;
use pley_core::model::ServerEvent;
use pley_ui::theme::{self, focus::PILL};
use pley_ui::{Motion, Ui};

use crate::cx::AppCx;
use crate::route::Route;

/// How a page shows around itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Chrome {
    /// Beside the rail, which floats over the page's left 72 dp (the page lays out from x = 96;
    /// the shell paints the rail's fade over the page's left edge). The rail shows when the back
    /// stack's first page has it too.
    Rail,
    /// Full screen without the rail: getting in, "Who's watching?", the pads.
    Bare,
    /// A player: no rail, toasts lower (y 79), never dimmed; the page clears to transparent where
    /// the video shows.
    Video,
}

/// What the shell tells a page besides drawing it.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    /// Something happened on the server (its event stream).
    Server(ServerEvent),
    /// What the server holds changed (its events, a playback ending, the server back after being
    /// out of reach): pages that list things ask again. A page under the top hears it when it is
    /// on top again.
    Changed,
    /// Live TV's channels or guide changed: its pages ask again.
    LiveChanged,
    /// The app went to the background (`false`) or came back (`true`).
    Visible(bool),
    /// The page is on top again: Back from the page above it, the player closing.
    Shown,
    /// The server ended the sign-in; the shell shows signing in again.
    SignedOut,
    /// News from the TV: its screen reader, its network.
    Device(DeviceEvent),
}

/// A page of the app (DESIGN §3.5). Screens are kept on the back stack while pages above them
/// show, so they keep their state; only the top one draws.
pub trait Screen {
    /// Draws the page and handles what was done to it this frame.
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx);

    /// Back was pressed: closes something of the page's own (a panel, a dialog, the keyboard, a
    /// sorting mode, Home's rows back to the hero) and says so with `true`; otherwise the shell
    /// pops the page, or opens the rail on a first page, or leaves the app.
    fn back(&mut self, _cx: &mut AppCx) -> bool {
        false
    }

    /// Hears what the shell tells pages ([`AppEvent`]).
    fn event(&mut self, _event: &AppEvent, _cx: &mut AppCx) {}

    /// How the page shows.
    fn chrome(&self) -> Chrome;

    /// Whether the page has something to focus now. A page chosen in the rail takes focus from
    /// the rail once it has (until then the rail keeps it, open, as Android's drawer does); a page
    /// whose loaded state has nothing to focus sends focus to the rail itself
    /// ([`AppCx::to_rail`]).
    fn takes_focus(&self) -> bool {
        true
    }

    /// Whether the page has something open that keeps focus (a side panel, a dialog, the
    /// keyboard): Left at its edge, or the pointer over the rail, then leaves the rail closed.
    fn modal(&self) -> bool {
        false
    }
}

/// Makes the screen of a route, or says it doesn't draw that route.
pub type ScreenFactory = Box<dyn Fn(&Route, &mut AppCx) -> Option<Box<dyn Screen>>>;

/// Every page's screen: the app adds each screen crate's factory (a function
/// `fn(&Route, &mut AppCx) -> Option<Box<dyn Screen>>`); a route no factory draws gets the
/// [`PlaceholderScreen`].
#[derive(Default)]
pub struct Registry {
    factories: Vec<ScreenFactory>,
}

impl Registry {
    /// A registry with no screens: every route gets the placeholder.
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Adds a factory; earlier ones are asked first.
    pub fn add(&mut self, factory: impl Fn(&Route, &mut AppCx) -> Option<Box<dyn Screen>> + 'static) -> &mut Registry {
        self.factories.push(Box::new(factory));
        self
    }

    /// The screen for `route`: the first factory's that draws it, else the placeholder.
    pub fn make(&self, route: &Route, cx: &mut AppCx) -> Box<dyn Screen> {
        self.factories
            .iter()
            .find_map(|f| f(route, cx))
            .unwrap_or_else(|| Box::new(PlaceholderScreen::new(route.clone())))
    }

    /// Whether a factory draws `route` (rather than the placeholder).
    pub fn draws(&self, route: &Route, cx: &mut AppCx) -> bool {
        self.factories.iter().any(|f| f(route, cx).is_some())
    }
}

/// The page of a route whose screen isn't built yet: it names the route, and its one button does
/// what Back does.
#[derive(Debug)]
pub struct PlaceholderScreen {
    route: Route,
}

impl PlaceholderScreen {
    /// The placeholder of `route`.
    pub fn new(route: Route) -> PlaceholderScreen {
        PlaceholderScreen { route }
    }

    /// The route it stands in for.
    pub fn route(&self) -> &Route {
        &self.route
    }
}

/// What the placeholder's text says under its title.
pub const PLACEHOLDER_TEXT: &str = "This page isn't built yet.";

impl Screen for PlaceholderScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let x = if self.chrome() == Chrome::Rail { theme::GUTTER } else { theme::SAFE_X };
        let width = theme::CANVAS_W - theme::SAFE_X - x;
        let title = self.route.title();
        ui.text(Point::new(x, theme::SAFE_Y), &title, &theme::PAGE_TITLE, &TextOpts::line(width), theme::FG);
        let y = theme::SAFE_Y + theme::PAGE_TITLE.line_height + theme::space::S;
        let said = format!("{PLACEHOLDER_TEXT} ({})", self.route.name());
        ui.text(Point::new(x, y), &said, &theme::BODY, &TextOpts::wrap(width, 2), theme::MUTED);

        let label = "Go back";
        let text = ui.measure(label, &theme::LABEL, &TextOpts::default());
        let rect = Rect::new(x, y + 44.0, text.w + 2.0 * theme::BUTTON_LABEL_ROOM, theme::BUTTON_HEIGHT);
        let id = ui.id("back");
        ui.initial_focus(id);
        let r = ui.focusable(id, rect);
        let look = PILL;
        let s = ui.animate(id.with("scale"), if r.focused { look.focus_scale } else { 1.0 }, Motion::FOCUS);
        let fill = if r.pressed {
            look.pressed_fill
        } else if r.focused {
            look.focus_fill
        } else {
            look.fill
        };
        let content = if r.focused { look.focus_content } else { look.content };
        ui.transform(Point::ZERO, s, rect.center(), |ui| {
            let radius = rect.h / 2.0;
            ui.fill(rect, fill, radius);
            if let (Some(edge), false) = (look.edge, r.focused) {
                ui.stroke(rect, edge, radius, theme::focus::REST_EDGE);
            }
            let at = Point::new(rect.x + theme::BUTTON_LABEL_ROOM, rect.y + (rect.h - theme::LABEL.line_height) / 2.0);
            ui.text(at, label, &theme::LABEL, &TextOpts::default(), content);
        });
        if r.activated {
            cx.back();
        }
    }

    fn chrome(&self) -> Chrome {
        self.route.chrome()
    }
}
