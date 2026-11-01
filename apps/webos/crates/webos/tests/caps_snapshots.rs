//! The web app's capability snapshots (`apps/webos/test/caps.test.ts` and
//! its `__snapshots__/caps.test.ts.snap`, kept here byte for byte): for the
//! same facts the native report must print exactly as the web app's did,
//! in Vitest's snapshot format (keys sorted, a comma after every entry).

use pley_core::device::CapsOverrides;
use pley_webos::caps::webos_caps;
use pley_webos::facts::WebosFacts;
use serde_json::{Value, json};

const SNAPSHOTS: &str = include_str!("caps.test.ts.snap");

/// The web app's test set: a 2025 OLED with its speakers, changed by `over`.
fn tv(over: impl FnOnce(&mut WebosFacts)) -> WebosFacts {
    let mut f = WebosFacts {
        webos: 25,
        release: None,
        build: None,
        model: "OLED65C5PUA".into(),
        firmware: Some("33.20.65".into()),
        screen: Some((3840, 2160)),
        uhd: true,
        uhd8k: false,
        oled: true,
        hdr10: true,
        dolby_vision: true,
        sound_output: Some("tv_speaker".into()),
        digital_output: Some("auto".into()),
        atmos: false,
        edid: None,
        memory: Some("3G".into()),
        codecs: None,
        clock_24h: true,
    };
    over(&mut f);
    f
}

/// Real sets, as their services describe them (`caps.test.ts`' `SETS`).
fn sets() -> Vec<(&'static str, WebosFacts)> {
    let s = |v: &str| Some(v.to_string());
    vec![
        (
            "2018 OLED C8, webOS 4, TV speakers",
            tv(|f| {
                f.webos = 4;
                f.model = "OLED55C8PUA".into();
                f.firmware = s("04.40.10");
                f.memory = s("2.5G");
            }),
        ),
        (
            "2019 LCD UM7300, webOS 4.5, no Dolby Vision, soundbar on ARC",
            tv(|f| {
                f.webos = 4;
                f.model = "43UM7300PUA".into();
                f.oled = false;
                f.dolby_vision = false;
                f.sound_output = s("external_arc");
                f.memory = s("1.25G");
            }),
        ),
        (
            "2020 OLED CX, webOS 5, Atmos receiver passing through",
            tv(|f| {
                f.webos = 5;
                f.model = "OLED65CXPUA".into();
                f.sound_output = s("external_arc");
                f.digital_output = s("passThrough");
                f.atmos = true;
                f.edid = s("TrueHD");
            }),
        ),
        (
            "2022 OLED C2 upgraded to webOS 25, DTS receiver",
            tv(|f| {
                f.webos = 25;
                f.model = "OLED55C2PUA".into();
                f.sound_output = s("external_arc");
                f.edid = s("TrueHD+dts");
            }),
        ),
        (
            "2023 OLED C3, webOS 23, DTS receiver",
            tv(|f| {
                f.webos = 23;
                f.model = "OLED65C3PUA".into();
                f.sound_output = s("external_arc");
                f.edid = s("TrueHD+dts");
            }),
        ),
        (
            "2026 OLED C6, webOS 26",
            tv(|f| {
                f.webos = 26;
                f.model = "OLED65C6PUA".into();
            }),
        ),
        (
            "2021 HD set, webOS 6, PCM output",
            tv(|f| {
                f.webos = 6;
                f.model = "32LM630BPUA".into();
                f.screen = Some((1920, 1080));
                f.uhd = false;
                f.oled = false;
                f.hdr10 = false;
                f.dolby_vision = false;
                f.sound_output = s("external_optical");
                f.digital_output = s("pcm");
                f.memory = s("1G");
            }),
        ),
        (
            "2020 8K NanoCell, webOS 5",
            tv(|f| {
                f.webos = 5;
                f.model = "75NANO99UNA".into();
                f.oled = false;
                f.uhd8k = true;
                f.screen = Some((7680, 4320));
            }),
        ),
    ]
}

/// The snapshot named `name` in the file.
fn snapshot(name: &str) -> &'static str {
    let head = format!("exports[`LG capabilities, per set > {name} 1`] = `\n");
    let start = SNAPSHOTS.find(&head).unwrap_or_else(|| panic!("no snapshot for {name}")) + head.len();
    let end = SNAPSHOTS[start..].find("\n`;").expect("an unterminated snapshot");
    &SNAPSHOTS[start..start + end]
}

/// `value` as Vitest's pretty-format prints it.
fn pretty(value: &Value, indent: usize, out: &mut String) {
    let pad = |n: usize, out: &mut String| out.extend(std::iter::repeat_n(' ', n));
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push_str("[\n");
            for item in items {
                pad(indent + 2, out);
                pretty(item, indent + 2, out);
                out.push_str(",\n");
            }
            pad(indent, out);
            out.push(']');
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push_str("{\n");
            for key in keys {
                pad(indent + 2, out);
                out.push_str(&serde_json::to_string(key).unwrap());
                out.push_str(": ");
                pretty(&map[key], indent + 2, out);
                out.push_str(",\n");
            }
            pad(indent, out);
            out.push('}');
        }
    }
}

#[test]
fn every_set_reports_byte_for_byte_as_the_web_app() {
    let sets = sets();
    assert_eq!(SNAPSHOTS.matches("exports[`").count(), sets.len(), "a snapshot without a set");
    for (name, facts) in sets {
        let report = webos_caps(&facts, &CapsOverrides::default());
        let value =
            json!({ "caps": serde_json::to_value(&report.caps).unwrap(), "dvContainers": report.dv_containers });
        let mut printed = String::new();
        pretty(&value, 0, &mut printed);
        assert_eq!(printed, snapshot(name), "{name}");
    }
}

#[test]
fn never_claims_hdr10_plus_which_no_lg_shows() {
    for (_, facts) in sets() {
        let json = serde_json::to_string(&webos_caps(&facts, &CapsOverrides::default()).caps).unwrap();
        assert!(!json.contains("hdr10plus"));
        assert!(!json.contains("variants"), "one size per stream: LG's pipeline may go black between sizes");
    }
}
