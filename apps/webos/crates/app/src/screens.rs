//! Every page's screen: each screen crate's factory (`fn(&Route, &mut AppCx) ->
//! Option<Box<dyn Screen>>`) goes into the registry here; a route no factory draws shows the
//! shell's placeholder.

use pley_shell::Registry;

/// The registry of every screen the app has.
pub fn registry() -> Registry {
    let mut screens = Registry::new();
    screens
        .add(pley_onboarding::screen)
        .add(pley_home::screen)
        .add(pley_browse::screen)
        .add(pley_item::screen)
        .add(pley_search::screen)
        .add(pley_music::screen)
        .add(pley_settings::screen)
        .add(pley_live::screen)
        .add(pley_requests::screen)
        .add(pley_storage::screen)
        .add(pley_player_ui::screen)
        .tile_actions(pley_item::tile_actions);
    screens
}

#[cfg(test)]
mod tests {
    use pley_core::media::NullEngine;
    use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
    use pley_shell::{AppCx, AppState, Requests, Route};

    fn state() -> AppState {
        let device = FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>).shared();
        let store = Store::open(device.data_dir().join("store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        AppState::new(device, store, http, Tasks::new(pley_base::Waker::noop()))
    }

    #[test]
    fn every_route_has_a_screen() {
        let mut state = state();
        let mut requests = Requests::default();
        let registry = super::registry();
        for route in Route::samples() {
            let mut cx = AppCx::new(&mut state, &mut requests, 0.0);
            let screen = registry.make(&route, &mut cx);
            assert_eq!(screen.chrome(), route.chrome(), "{}", route.name());
        }
    }
}
