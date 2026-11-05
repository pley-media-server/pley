import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Settings } from "@/api/types";
import { mockApi, renderWithProviders } from "@/test/render";
import { TranscoderSettings } from "./TranscoderSettings";

const SETTINGS = {
  server_name: "Den",
  language: "",
  tmdb_api_key: "",
  metadata_language: "en-US",
  tv_metadata: "auto",
  tvdb_api_key: "",
  tvdb_pin: "",
  music_metadata: true,
  hwaccel: "auto",
  hardware_decoding: true,
  tonemapping: "auto",
  max_transcodes: 4,
  transcode_preset: "veryfast",
  throttle_transcodes: true,
  transcode_in_memory: true,
  scan_interval_minutes: 360,
  watch_libraries: true,
  detect_markers: true,
  seek_previews: true,
  previews_in_background: true,
  third_party_apps: false,
} satisfies Settings;

describe("TranscoderSettings' form", () => {
  it("saves its own settings only", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => ({}),
      "PUT /api/settings": (req) => ({ ...SETTINGS, ...(req.body as Partial<Settings>) }),
    });
    renderWithProviders(<TranscoderSettings />);
    const throttle = await screen.findByRole("switch", { name: "Throttle transcodes" });
    expect(screen.queryByLabelText("Server name")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    await user.click(throttle);
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ throttle_transcodes: false }));
    expect(screen.getByRole("switch", { name: "Throttle transcodes" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });
});

describe("TranscoderSettings' folder", () => {
  const transcoder = { available: true, managed: true, encoders: [], hwaccels: [], tonemapping: ["zscale"], dolby_vision: false };
  const folders = { in_use: "C:\\pley\\transcode", default: "C:\\pley\\transcode", problem: null, fixed: false };

  it("chooses the transcode folder for pley's next start, and goes back to the default", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => ({ transcoder, transcode_dir: { ...folders, next: "D:\\fast" } }),
      "PUT /api/system/transcode-dir": () => ({ ...folders, next: null }),
    });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByText("D:\\fast")).toBeInTheDocument();
    expect(screen.getByText(/From Pley's next start/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Use the default" }));
    expect(await screen.findByText("Saved")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ path: null });
    expect(screen.queryByText(/From Pley's next start/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Use the default" })).not.toBeInTheDocument();
  });

  it("says why the folder chosen isn't used, and where to get Dolby Vision", async () => {
    mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => ({
        transcoder,
        transcode_dir: { ...folders, next: null, problem: { folder: "E:\\gone", reason: "The device is not ready." } },
      }),
    });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByText("E:\\gone")).toBeInTheDocument();
    expect(screen.getByText(/The device is not ready\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use the default" })).toBeInTheDocument();
    expect(screen.getByText(/Dolby Vision needs an FFmpeg with libplacebo/)).toBeInTheDocument();
  });

  it("leaves a folder the command line chooses alone", async () => {
    mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => ({ transcoder, transcode_dir: { ...folders, next: null, fixed: true } }),
    });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByText(/PLEY_TRANSCODE_DIR/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Change…" })).not.toBeInTheDocument();
  });
});

describe("TranscoderSettings on this server's hardware", () => {
  /** pley on Windows with an Intel graphics chip, and an FFmpeg without libplacebo or -readrate. */
  const intel = {
    available: true,
    managed: true,
    max_jobs: 8,
    encoders: ["h264_qsv", "libx264"],
    active_encoder: "h264_qsv",
    hwaccels: ["qsv", "d3d11va"],
    accels: ["qsv"],
    hardware_decoding: true,
    tonemapping: ["opencl", "zscale"],
    dolby_vision: false,
    chromaprint: false,
    throttling: false,
  };
  const system = (transcoder: object, os = "windows") => ({ os, transcoder });
  const option = (name: string | RegExp) => screen.getByRole("option", { name });

  it("greys out what this computer can't do, and says why", async () => {
    mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => system(intel) });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByRole("option", { name: "Automatic · Intel Quick Sync" })).toBeEnabled();
    expect(option(/^Intel Quick Sync$/)).toBeEnabled();
    // Greyed out with what they need.
    expect(option("NVIDIA NVENC (needs an NVIDIA graphics card)")).toBeDisabled();
    expect(option("AMD AMF (needs an AMD graphics card)")).toBeDisabled();
    // Never on Windows: not offered at all.
    expect(screen.queryByRole("option", { name: /VAAPI|VideoToolbox/ })).not.toBeInTheDocument();

    expect(option(/^Automatic · OpenCL$/)).toBeEnabled();
    expect(option("libplacebo (needs libplacebo and a Vulkan GPU)")).toBeDisabled();
    expect(option(/^zscale/)).toBeEnabled();

    // The current values stay, greyed out with the reason.
    expect(screen.getByRole("switch", { name: "Throttle transcodes" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "Throttle transcodes" })).toBeChecked();
    expect(screen.getByText(/it needs FFmpeg 6\.1 or newer/)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Hardware decoding" })).toBeEnabled();
    // The preset is for software encoding only.
    expect(screen.getByText("Intel Quick Sync encodes here: this applies only when software encodes instead.")).toBeInTheDocument();
  });

  it("says what a choice saved before does here instead", async () => {
    mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, hwaccel: "nvenc", tonemapping: "libplacebo" }),
      "GET /api/system": () => system({ ...intel, active_encoder: "libx264", hardware_decoding: false }),
    });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByLabelText("Hardware acceleration")).toHaveValue("nvenc");
    expect(screen.getByText("Needs an NVIDIA graphics card: transcodes are encoded in software instead.")).toBeInTheDocument();
    expect(screen.getByLabelText("HDR tone mapping")).toHaveValue("libplacebo");
    expect(screen.getByText("Needs libplacebo and a Vulkan GPU: HDR is tone mapped with OpenCL instead.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Hardware decoding" })).toBeDisabled();
    expect(screen.getByText(/videos are decoded on the CPU/)).toBeInTheDocument();
  });

  it("keeps a choice this system never has when it was saved", async () => {
    mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, hwaccel: "vaapi" }),
      "GET /api/system": () => system({ ...intel, available: false, accels: [], encoders: [] }),
    });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByLabelText("Hardware acceleration")).toHaveValue("vaapi");
    expect(option("VAAPI (on Linux only)")).toBeDisabled();
    expect(screen.getByText("On Linux only: transcodes are encoded in software instead.")).toBeInTheDocument();
    // The transcoder isn't running: what its FFmpeg can do isn't known, so nothing else is greyed out.
    expect(option(/^NVIDIA NVENC$/)).toBeEnabled();
    expect(within(screen.getByLabelText("Hardware acceleration")).getByRole("option", { name: /^Automatic$/ })).toBeEnabled();
    expect(screen.getByRole("switch", { name: "Throttle transcodes" })).toBeEnabled();
  });

  it("judges a transcoder elsewhere by its encoders alone", async () => {
    mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => system({ ...intel, managed: false, url: "http://gpu-box:7540", accels: ["vaapi"] }),
    });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByRole("option", { name: "Automatic · VAAPI" })).toBeEnabled();
    expect(option(/^VAAPI$/)).toBeEnabled();
    expect(option("VideoToolbox (needs an FFmpeg with VideoToolbox)")).toBeDisabled();
  });

  describe("in memory", () => {
    const memorySwitch = () => screen.findByRole("switch", { name: "Transcode in memory" });
    /** pley on Linux with 16 GB, whose FFmpeg throttles. */
    const linux = {
      ...intel,
      throttling: true,
      memory: { available: true, path: "/dev/shm", budget_bytes: 4 * 1024 ** 3 },
    };

    it("is on by default where the transcoder has a folder in RAM, and turns off", async () => {
      const user = userEvent.setup();
      const requests = mockApi({
        "GET /api/settings": () => SETTINGS,
        "GET /api/system": () => system(linux, "linux"),
        "PUT /api/settings": (req) => ({ ...SETTINGS, ...(req.body as Partial<Settings>) }),
      });
      renderWithProviders(<TranscoderSettings />);
      const toggle = await memorySwitch();
      expect(toggle).toBeEnabled();
      expect(toggle).toBeChecked();
      expect(screen.getByText("Keeps segments in RAM instead of on disk. Up to 4.0 GB.")).toBeInTheDocument();
      expect(screen.queryByText(/segments go to disk/)).not.toBeInTheDocument();

      await user.click(toggle);
      await user.click(screen.getByRole("button", { name: "Save" }));
      await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ transcode_in_memory: false }));
    });

    it("is greyed out where the transcoder has none, and says why", async () => {
      const windows = { ...intel, throttling: true, memory: { available: false, reason: "works on Linux only", budget_bytes: 0 } };
      mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => system(windows) });
      renderWithProviders(<TranscoderSettings />);
      const toggle = await memorySwitch();
      expect(toggle).toBeDisabled();
      expect(toggle).toBeChecked();
      expect(screen.getByText("Works on Linux only: segments go to disk.")).toBeInTheDocument();
      expect(screen.getByText("Keeps segments in RAM instead of on disk.")).toBeInTheDocument();
    });

    it("needs throttling", async () => {
      const user = userEvent.setup();
      mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => system(linux, "linux") });
      renderWithProviders(<TranscoderSettings />);
      expect(await memorySwitch()).toBeEnabled();
      await user.click(screen.getByRole("switch", { name: "Throttle transcodes" }));
      expect(screen.getByRole("switch", { name: "Transcode in memory" })).toBeDisabled();
      expect(screen.getByText("Needs throttling: segments go to disk.")).toBeInTheDocument();
    });

    it("needs an FFmpeg that throttles", async () => {
      mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => system({ ...linux, throttling: false }, "linux") });
      renderWithProviders(<TranscoderSettings />);
      expect(await memorySwitch()).toBeDisabled();
      expect(screen.getByText("Needs throttling: segments go to disk.")).toBeInTheDocument();
    });

    it("is left alone while the transcoder isn't running", async () => {
      mockApi({
        "GET /api/settings": () => SETTINGS,
        "GET /api/system": () => system({ ...intel, available: false, accels: [], encoders: [], memory: null }),
      });
      renderWithProviders(<TranscoderSettings />);
      expect(await memorySwitch()).toBeEnabled();
      expect(screen.queryByText(/segments go to disk/)).not.toBeInTheDocument();
    });
  });

  describe("with the hardware acceleration chosen", () => {
    const accelSelect = () => screen.getByLabelText("Hardware acceleration");
    const decoding = () => screen.getByRole("switch", { name: "Hardware decoding" });
    /** Quick Sync decodes through D3D11 and encodes H.264; x264 gets no decoder here. */
    const withAccel = {
      ...intel,
      with_accel: {
        auto: { hardware_decoding: true, software: ["hevc", "hdr"] },
        qsv: { hardware_decoding: true, software: ["hevc", "hdr"] },
        nvenc: { hardware_decoding: false, software: ["h264", "hevc", "hdr"] },
        none: { hardware_decoding: false, software: ["h264", "hevc", "hdr"] },
      },
    };

    it("greys out hardware decoding where it decodes nothing on a GPU, and says what the preset applies to", async () => {
      const user = userEvent.setup();
      mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => system(withAccel) });
      renderWithProviders(<TranscoderSettings />);
      expect(await screen.findByText("Intel Quick Sync encodes H.264 here: this applies to HEVC, and when software encodes instead.")).toBeInTheDocument();
      expect(decoding()).toBeEnabled();

      await user.selectOptions(accelSelect(), "none");
      expect(decoding()).toBeDisabled();
      expect(decoding()).toBeChecked();
      expect(screen.getByText("Not with software encoding: videos are decoded on the CPU.")).toBeInTheDocument();
      expect(screen.queryByText(/encodes H\.264 here/)).not.toBeInTheDocument();

      await user.selectOptions(accelSelect(), "qsv");
      expect(decoding()).toBeEnabled();
      expect(screen.queryByText(/videos are decoded on the CPU/)).not.toBeInTheDocument();
    });

    it("leaves hardware decoding alone on a server that doesn't say what each choice decodes", async () => {
      const user = userEvent.setup();
      mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => system(intel) });
      renderWithProviders(<TranscoderSettings />);
      await user.selectOptions(await screen.findByLabelText("Hardware acceleration"), "none");
      expect(decoding()).toBeEnabled();
    });

    describe("Vulkan", () => {
      const vulkan = { ...withAccel, tonemapping: ["libplacebo", "opencl", "zscale"], dolby_vision: true };
      const vulkanSaved = { ...SETTINGS, hwaccel: "vulkan", tonemapping: "libplacebo" } satisfies Settings;

      it("tone maps with libplacebo and decodes on the GPU, and what doesn't go with it falls back visibly", async () => {
        const user = userEvent.setup();
        const requests = mockApi({
          "GET /api/settings": () => vulkanSaved,
          "GET /api/system": () => system(vulkan),
          "PUT /api/settings": (req) => ({ ...vulkanSaved, ...(req.body as Partial<Settings>) }),
        });
        renderWithProviders(<TranscoderSettings />);
        expect(await screen.findByRole("option", { name: "OpenCL (not with Vulkan)" })).toBeDisabled();
        expect(option("zscale (not with Vulkan)")).toBeDisabled();
        expect(option("Disabled (not with Vulkan)")).toBeDisabled();
        expect(option(/^libplacebo$/)).toBeEnabled();
        expect(option("Automatic · libplacebo")).toBeEnabled();
        expect(decoding()).toBeDisabled();
        expect(decoding()).toBeChecked();
        expect(screen.getByText("Always on with Vulkan, which decodes on the GPU.")).toBeInTheDocument();

        // With another, they can be chosen.
        await user.selectOptions(accelSelect(), "qsv");
        expect(option(/^zscale$/)).toBeEnabled();
        await user.selectOptions(screen.getByLabelText("HDR tone mapping"), "zscale");
        await user.click(decoding());
        expect(decoding()).not.toBeChecked();

        // Back to Vulkan, they give way, and say so.
        await user.selectOptions(accelSelect(), "vulkan");
        expect(screen.getByLabelText("HDR tone mapping")).toHaveValue("auto");
        expect(screen.getByText("Set to Automatic: Vulkan tone maps with libplacebo only.")).toBeInTheDocument();
        expect(decoding()).toBeChecked();
        expect(decoding()).toBeDisabled();
        expect(screen.getByText("Turned on: Vulkan decodes on the GPU.")).toBeInTheDocument();

        await user.click(screen.getByRole("button", { name: "Save" }));
        await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ tonemapping: "auto" }));
        expect(screen.queryByText(/Set to Automatic/)).not.toBeInTheDocument();
      });

      it("says what a combination saved before does", async () => {
        mockApi({
          "GET /api/settings": () => ({ ...vulkanSaved, tonemapping: "zscale", hardware_decoding: false }),
          "GET /api/system": () => system(vulkan),
        });
        renderWithProviders(<TranscoderSettings />);
        expect(await screen.findByLabelText("HDR tone mapping")).toHaveValue("zscale");
        expect(screen.getByText("Not with Vulkan: transcodes are encoded in software instead.")).toBeInTheDocument();
        // It can be turned on, not off.
        expect(decoding()).toBeEnabled();
        expect(screen.getByText("Vulkan needs it: transcodes are encoded in software instead.")).toBeInTheDocument();
      });
    });
  });

  it("says when the transcoder runs fewer at once than allowed", async () => {
    mockApi({ "GET /api/settings": () => ({ ...SETTINGS, max_transcodes: 12 }), "GET /api/system": () => system(intel) });
    renderWithProviders(<TranscoderSettings />);
    expect(await screen.findByText(/The transcoder itself runs 8 at most/)).toBeInTheDocument();
  });
});
