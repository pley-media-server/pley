import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ResourceClient, ResourceSample, Resources } from "@/api/types";
import { mockApi, renderWithProviders } from "@/test/render";
import { networkScale, ResourcesSection } from "./ResourcesSection";

const GB = 1024 ** 3;

function sample(at: number, pley_cpu: number, cpu: number): ResourceSample {
  return { at, cpu, pley_cpu, memory: 8 * GB, pley_memory: GB / 2, net_in: 2_000_000, net_out: 5_000_000, pley_out: 4_500_000 };
}

function resources(samples: ResourceSample[], available = true, clients: ResourceClient[] = []): Resources {
  return { available, interval_ms: 2_000, cpus: 8, memory_total: 32 * GB, samples, clients };
}

describe("ResourcesSection", () => {
  it("shows pley's and the machine's processors and memory, then asks only for newer samples", async () => {
    const answers = [resources([sample(1_000, 2.5, 20), sample(3_000, 12, 34)]), resources([sample(5_000, 40, 61)])];
    const requests = mockApi({ "GET /api/system/resources": () => answers.shift() ?? resources([]) });
    renderWithProviders(<ResourcesSection />);

    const cpu = await screen.findByRole("img", { name: "CPU: Pley 12%, System 34%" });
    expect(cpu).toBeInTheDocument();
    expect(screen.getByText("8 processors")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Memory: Pley 512 MB, System 8.0 GB" })).toBeInTheDocument();
    expect(screen.getByText("32.0 GB in all")).toBeInTheDocument();

    // The next poll carries the newest sample's time, and its answer joins the others.
    expect(await screen.findByRole("img", { name: "CPU: Pley 40%, System 61%" }, { timeout: 4_000 })).toBeInTheDocument();
    const polls = requests.filter((r) => r.path.startsWith("/api/system/resources"));
    expect(polls[0]?.path).toBe("/api/system/resources");
    expect(polls[1]?.path).toBe("/api/system/resources?since=3000");
  });

  it("shows the network in bits a second, and who Pley is sending to", async () => {
    const clients: ResourceClient[] = [
      { user: "ana", device: "Pley for Roku", address: "203.0.113.5", sent: 4_000_000 },
      { user: null, device: null, address: "198.51.100.9", sent: 500_000 },
    ];
    mockApi({ "GET /api/system/resources": () => resources([sample(1_000, 2.5, 20)], true, clients) });
    renderWithProviders(<ResourcesSection />);

    expect(await screen.findByRole("img", { name: "Network: Pley 36 Mbps, Upload 40 Mbps, Download 16 Mbps" })).toBeInTheDocument();
    expect(screen.getByText("Fastest 40 Mbps")).toBeInTheDocument();
    // The scale rounds the fastest up: 40 Mbps to 50.
    expect(screen.getByText("50 Mbps")).toBeInTheDocument();
    expect(screen.getByText("25 Mbps")).toBeInTheDocument();

    const list = screen.getByRole("list", { name: "Clients" });
    expect(list).toHaveTextContent("ana32 MbpsPley for Roku · 203.0.113.5");
    expect(list).toHaveTextContent("198.51.100.94.0 Mbps");
    expect(screen.getByText("2 now")).toBeInTheDocument();
  });

  it("says when nobody is being sent anything", async () => {
    mockApi({ "GET /api/system/resources": () => resources([sample(1_000, 2.5, 20)]) });
    renderWithProviders(<ResourcesSection />);
    expect(await screen.findByText("Pley isn't sending anything right now.")).toBeInTheDocument();
  });

  it("scales the network to the fastest, 1 Mbps at least", () => {
    const at = (net_in: number) => ({ ...sample(0, 0, 0), net_in, net_out: 0, pley_out: 0 });
    expect(networkScale([])).toBe(125_000);
    expect(networkScale([at(1_000)])).toBe(125_000);
    expect(networkScale([at(150_000)])).toBe(250_000);
    expect(networkScale([at(12_500_000), at(400_000)])).toBe(12_500_000);
    expect(networkScale([at(12_600_000)])).toBe(25_000_000);
  });

  it("reads out each sample from the keyboard", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/system/resources": () => resources([sample(1_000, 2.5, 20), sample(3_000, 12, 34)]) });
    renderWithProviders(<ResourcesSection />);
    const cpu = await screen.findByRole("img", { name: /^CPU/ });
    await user.click(cpu);
    await user.keyboard("{ArrowLeft}");
    await waitFor(() => expect(cpu).toHaveTextContent("2.5%Pley20%System"));
    await user.keyboard("{Escape}");
    await waitFor(() => expect(cpu).not.toHaveTextContent("Pley"));
  });

  it("says so where the system can't be measured", async () => {
    mockApi({ "GET /api/system/resources": () => resources([], false) });
    renderWithProviders(<ResourcesSection />);
    expect(await screen.findByText("The processors and memory can't be measured on this system.")).toBeInTheDocument();
  });
});
