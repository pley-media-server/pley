import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ApiError } from "@/api/client";
import { renderWithProviders } from "@/test/render";
import { makeRequest, makeTitle } from "@/test/requests";
import { ProblemState, StagePill, TitleCard } from "./shared";

const now = () => Math.floor(Date.now() / 1000);

describe("TitleCard", () => {
  it("links to the title's page, saying where it stands in its name and on its poster", () => {
    renderWithProviders(<TitleCard title={makeTitle({ status: "requested" })} width={150} />);
    expect(screen.getByRole("link", { name: "The Matrix, 1999, Movie, Requested" })).toHaveAttribute("href", "/discover/movie/603");
    expect(screen.getByText("Requested")).toBeInTheDocument();
    expect(screen.getByText("1999 · Movie")).toBeInTheDocument();
  });

  it("asks the server for the poster at the size it's drawn, lazily", () => {
    renderWithProviders(<TitleCard title={makeTitle()} width={150} />);
    const img = document.querySelector("img");
    expect(img).toHaveAttribute("src", "/api/discover/images/poster/matrix.jpg?w=160");
    expect(img).toHaveAttribute("loading", "lazy");
  });

  it("has no badge for titles that may be asked for", () => {
    renderWithProviders(<TitleCard title={makeTitle()} width={150} />);
    expect(screen.getByRole("link", { name: "The Matrix, 1999, Movie" })).toBeInTheDocument();
    expect(screen.queryByText("Requested")).not.toBeInTheDocument();
  });
});

describe("StagePill", () => {
  it("waits for approval in amber", () => {
    renderWithProviders(<StagePill request={makeRequest()} manage={false} />);
    expect(screen.getByText("Waiting for approval").parentElement).toHaveClass("bg-warning/15");
  });

  it("shows a download's progress as a bar with its value, and when it should be done", () => {
    const request = makeRequest({
      status: "approved",
      stage: "downloading",
      download: { progress: 0.42, state: "downloading", eta: now() + 20 * 60, episodes: 1 },
    });
    renderWithProviders(<StagePill request={request} manage={false} />);
    expect(screen.getByText("Downloading 42% · in 20 min").parentElement).toHaveClass("bg-info/15");
    const bar = screen.getByRole("progressbar", { name: "The Matrix: Downloading 42% · in 20 min" });
    expect(bar).toHaveAttribute("aria-valuenow", "42");
  });

  it("tells managers why one failed, and others that an administrator will look at it", () => {
    const failed = makeRequest({ status: "failed", reason: "no quality profile is chosen" });
    const { unmount } = renderWithProviders(<StagePill request={failed} manage />);
    expect(screen.getByText("Couldn't send").parentElement).toHaveClass("bg-danger/15");
    expect(screen.getByText("no quality profile is chosen")).toBeInTheDocument();
    unmount();

    renderWithProviders(<StagePill request={failed} manage={false} />);
    expect(screen.getByText("Not sent yet")).toBeInTheDocument();
    expect(screen.getByText("It couldn't be sent to download yet: an administrator will look at it.")).toBeInTheDocument();
    expect(screen.queryByText(/quality profile/)).not.toBeInTheDocument();
  });

  it("says when a failed one is tried again", () => {
    renderWithProviders(<StagePill request={makeRequest({ status: "failed", retry_at: now() + 5 * 60 })} manage={false} />);
    expect(screen.getByText("Trying again in 5 min.")).toBeInTheDocument();
  });

  it("is green once ready, and gray once gone", () => {
    const { unmount } = renderWithProviders(<StagePill request={makeRequest({ status: "available", item_id: 9 })} manage={false} />);
    expect(screen.getByText("Ready to watch").parentElement).toHaveClass("bg-success/15");
    unmount();
    renderWithProviders(<StagePill request={makeRequest({ status: "available", stage: "gone" })} manage={false} />);
    expect(screen.getByText("No longer in a library").parentElement).toHaveClass("bg-white/10");
  });
});

describe("ProblemState", () => {
  const refused = new ApiError(503, "tmdb_key", "TMDB refused the key", { code: "tmdb_key", message: "TMDB refused the key" });

  it("sends managers to fix TMDB's key, with no Retry", () => {
    renderWithProviders(<ProblemState error={refused} manage onRetry={() => {}} />);
    expect(screen.getByRole("heading", { name: "TMDB refused this server's key" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Check the key" })).toHaveAttribute("href", "/settings/server#tmdb-key");
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("offers Retry where it may help, and says what failed for other errors", () => {
    const { unmount } = renderWithProviders(<ProblemState problem={{ kind: "tmdb_down" }} manage={false} onRetry={() => {}} />);
    expect(screen.getByRole("heading", { name: "TMDB isn't answering" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    unmount();

    renderWithProviders(<ProblemState error={new Error("the network is down")} manage={false} title="Couldn't load Discover" onRetry={() => {}} />);
    expect(screen.getByRole("heading", { name: "Couldn't load Discover" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});
