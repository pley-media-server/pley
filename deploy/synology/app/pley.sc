[pley]
title="Pley"
desc="Pley media server: the web app and the apps"
port_forward="yes"
dst.ports="7539/tcp,udp"

[pley_jellyfin_discovery]
title="Pley (Jellyfin discovery)"
desc="Pley media server: Jellyfin's apps find it on the local network"
port_forward="no"
dst.ports="7359/udp"

[pley_bonjour]
title="Pley (Bonjour)"
desc="Pley media server: Apple's apps find it on the local network"
port_forward="no"
dst.ports="5353/udp"
