# pley-transcoder

The FFmpeg media engine of the [Pley](../../README.md) media server: a
standalone HTTP service that does

- probing,
- HLS transcoding and remuxing, and converted downloads,
- HDR10, HLG and Dolby Vision tone mapping,
- subtitle burn-in and extraction,
- thumbnails and cover art,
- metrics in JSON and Prometheus formats.

It is under the AGPL-3.0, as all of Pley is ([LICENSE](../../LICENSE)), and
the Pley server talks to it only over HTTP
([LICENSING.md](../../LICENSING.md)).

## Run it

```sh
export PLEY_TRANSCODER_TOKEN=$(openssl rand -hex 16)
cargo run -p pley-transcoder -- --listen 127.0.0.1:7540
curl -H "Authorization: Bearer $PLEY_TRANSCODER_TOKEN" http://127.0.0.1:7540/v1/info
```

Pass the token in the environment, not with `--token`: every account on the
computer can read a command line in the list of processes.
`pley-transcoder --help` lists the other options.

[docs/TRANSCODER.md](../../docs/TRANSCODER.md) documents the protocol, the
HLS job model and the features.
