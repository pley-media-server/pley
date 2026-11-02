//! Every route, in the order they are tried: a literal path before a pattern
//! that would also match it (`/api/playback/network` before
//! `/api/playback/{session}`).

use crate::server::Route;

pub(crate) fn all() -> Vec<Route> {
    let mut routes = Vec::new();
    routes.extend(crate::oauth::routes());
    routes.extend(crate::extra::routes());
    routes.extend(crate::browse::routes());
    routes.extend(crate::play::routes());
    routes.extend(crate::lists::routes());
    routes.extend(crate::playlists::routes());
    routes.extend(crate::live::routes());
    routes.extend(crate::requests::routes());
    routes
}
