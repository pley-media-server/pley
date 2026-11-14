// SPDX-License-Identifier: AGPL-3.0-only
//! The GPUs this computer (or container) shows FFmpeg: the render node VAAPI
//! uses, and what [`crate::ffmpeg`]'s cached detection depends on besides
//! FFmpeg, so that a GPU given later (a Docker add-on, a driver installed) is
//! tried.

use std::sync::OnceLock;

/// The render node when nothing better is known.
const DEFAULT_RENDER_NODE: &str = "/dev/dri/renderD128";

/// The kernel drivers of GPUs VAAPI encodes on: Intel's and AMD's.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const VAAPI_DRIVERS: [&str; 4] = ["i915", "xe", "amdgpu", "radeon"];

/// The render node VAAPI uses: `PLEY_VAAPI_DEVICE`, else the first Intel or
/// AMD one (a computer with an NVIDIA card or two GPUs has several), else the
/// first one.
pub fn vaapi_device() -> &'static str {
    static DEVICE: OnceLock<String> = OnceLock::new();
    DEVICE.get_or_init(|| match std::env::var("PLEY_VAAPI_DEVICE") {
        Ok(device) if !device.trim().is_empty() => device.trim().to_owned(),
        _ => pick_render_node(&render_nodes()),
    })
}

/// The kernel drivers of Intel's GPUs, the only ones Quick Sync runs on.
const INTEL_DRIVERS: [&str; 2] = ["i915", "xe"];

/// The render node Quick Sync uses on Linux: `PLEY_VAAPI_DEVICE`, else the
/// first Intel one, else VAAPI's. FFmpeg's own pick is the first render
/// node, which is an NVIDIA or AMD GPU's on a computer that has one beside
/// Intel's.
pub fn qsv_device() -> &'static str {
    static DEVICE: OnceLock<String> = OnceLock::new();
    DEVICE.get_or_init(|| match std::env::var("PLEY_VAAPI_DEVICE") {
        Ok(device) if !device.trim().is_empty() => device.trim().to_owned(),
        _ => pick_intel_node(&render_nodes()).unwrap_or_else(|| vaapi_device().to_owned()),
    })
}

fn pick_intel_node(nodes: &[(String, Option<String>)]) -> Option<String> {
    nodes
        .iter()
        .find(|(_, driver)| driver.as_deref().is_some_and(|d| INTEL_DRIVERS.contains(&d)))
        .map(|(node, _)| format!("/dev/dri/{node}"))
}

/// What detection's results depend on besides FFmpeg: the render nodes and
/// their drivers, NVIDIA's driver, and WSL's GPU.
pub fn stamp() -> String {
    let nodes: Vec<String> =
        render_nodes().iter().map(|(node, driver)| format!("{node}={}", driver.as_deref().unwrap_or("?"))).collect();
    let nvidia = std::fs::read_to_string("/proc/driver/nvidia/version")
        .ok()
        .and_then(|text| text.lines().next().map(str::to_owned))
        .unwrap_or_default();
    let present = |path: &str| if std::path::Path::new(path).exists() { "1" } else { "0" };
    format!(
        "{}|vaapi={}|nvidia={}:{nvidia}|dxg={}",
        nodes.join(","),
        vaapi_device(),
        present("/dev/nvidia0"),
        present("/dev/dxg")
    )
}

fn pick_render_node(nodes: &[(String, Option<String>)]) -> String {
    nodes
        .iter()
        .find(|(_, driver)| driver.as_deref().is_some_and(|d| VAAPI_DRIVERS.contains(&d)))
        .or(nodes.first())
        .map(|(node, _)| format!("/dev/dri/{node}"))
        .unwrap_or_else(|| DEFAULT_RENDER_NODE.to_owned())
}

/// `/dev/dri`'s render nodes in order (`renderD128`, …), each with its
/// kernel driver. The driver is found by the node's device number, which
/// is the host's in a container too, where the node's name may not be.
#[cfg(target_os = "linux")]
fn render_nodes() -> Vec<(String, Option<String>)> {
    use std::os::unix::fs::MetadataExt;

    let Ok(entries) = std::fs::read_dir("/dev/dri") else {
        return Vec::new();
    };
    let mut nodes: Vec<(u32, String, Option<String>)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number = name.strip_prefix("renderD")?.parse().ok()?;
            let rdev = std::fs::metadata(entry.path()).ok()?.rdev();
            let driver = std::fs::read_link(format!("/sys/dev/char/{}:{}/device/driver", major(rdev), minor(rdev)))
                .ok()
                .and_then(|link| link.file_name().map(|n| n.to_string_lossy().into_owned()));
            Some((number, name, driver))
        })
        .collect();
    nodes.sort();
    nodes.into_iter().map(|(_, name, driver)| (name, driver)).collect()
}

#[cfg(not(target_os = "linux"))]
fn render_nodes() -> Vec<(String, Option<String>)> {
    Vec::new()
}

/// Linux's device number halves (glibc's `gnu_dev_major` and `gnu_dev_minor`).
#[cfg(target_os = "linux")]
fn major(dev: u64) -> u64 {
    ((dev & 0x0000_0000_000f_ff00) >> 8) | ((dev & 0xffff_f000_0000_0000) >> 32)
}

#[cfg(target_os = "linux")]
fn minor(dev: u64) -> u64 {
    (dev & 0x0000_0000_0000_00ff) | ((dev & 0x0000_0fff_fff0_0000) >> 12)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(list: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        list.iter().map(|(n, d)| ((*n).to_owned(), d.map(str::to_owned))).collect()
    }

    #[test]
    fn vaapi_takes_the_intel_or_amd_gpu() {
        // An NVIDIA card first, the processor's Intel graphics second.
        assert_eq!(
            pick_render_node(&nodes(&[("renderD128", Some("nvidia")), ("renderD129", Some("i915"))])),
            "/dev/dri/renderD129"
        );
        assert_eq!(
            pick_render_node(&nodes(&[("renderD128", Some("amdgpu")), ("renderD129", Some("xe"))])),
            "/dev/dri/renderD128"
        );
        // A container's one node, whatever its driver reads as.
        assert_eq!(pick_render_node(&nodes(&[("renderD128", None)])), "/dev/dri/renderD128");
        assert_eq!(pick_render_node(&[]), DEFAULT_RENDER_NODE);
    }

    #[test]
    fn quick_sync_takes_the_intel_gpu() {
        // AMD's first: VAAPI takes it, Quick Sync Intel's.
        let both = nodes(&[("renderD128", Some("amdgpu")), ("renderD129", Some("i915"))]);
        assert_eq!(pick_render_node(&both), "/dev/dri/renderD128");
        assert_eq!(pick_intel_node(&both).as_deref(), Some("/dev/dri/renderD129"));
        assert_eq!(pick_intel_node(&nodes(&[("renderD128", Some("xe"))])).as_deref(), Some("/dev/dri/renderD128"));
        // None known: VAAPI's node.
        assert_eq!(pick_intel_node(&nodes(&[("renderD128", None)])), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn device_numbers_split_as_glibc_does() {
        // glibc's gnu_dev_makedev.
        let makedev = |major: u64, minor: u64| {
            ((major & 0xfff) << 8) | ((major & !0xfff) << 32) | (minor & 0xff) | ((minor & !0xff) << 12)
        };
        for (ma, mi) in [(226, 128), (226, 129), (0x1234, 0x56789)] {
            let dev = makedev(ma, mi);
            assert_eq!((major(dev), minor(dev)), (ma, mi));
        }
    }
}
