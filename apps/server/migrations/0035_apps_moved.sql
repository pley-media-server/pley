-- Pley's own apps are known by client IDs on the pley-media-server GitHub
-- organisation now. The grants they hold under their first IDs move with
-- them, so no one has to sign in again once the app is updated.
UPDATE app_grants
SET client_id = 'https://pley-media-server.github.io/pley/oauth/'
    || substr(client_id, length('https://hhaidar.github.io/pley/oauth/') + 1)
WHERE client_id IN (
    'https://hhaidar.github.io/pley/oauth/android.json',
    'https://hhaidar.github.io/pley/oauth/desktop.json',
    'https://hhaidar.github.io/pley/oauth/kodi.json',
    'https://hhaidar.github.io/pley/oauth/roku.json',
    'https://hhaidar.github.io/pley/oauth/tizen.json',
    'https://hhaidar.github.io/pley/oauth/webos.json'
);
