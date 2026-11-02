import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Credit } from "@/api/types";
import { makeCredit } from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import { CastRow, CrewRows, hasCrew } from "./Credits";
import { personUrl } from "@/lib/urls";

describe("hasCrew", () => {
  it("counts creators, directors and writers", () => {
    expect(hasCrew(undefined)).toBe(false);
    expect(hasCrew([makeCredit(1, "Actor", "actor", "Don Lockwood")])).toBe(false);
    expect(hasCrew([makeCredit(2, "Producer", "producer")])).toBe(false);
    expect(hasCrew([makeCredit(3, "Director", "director")])).toBe(true);
  });
});

describe("CrewRows", () => {
  function renderCrew(crew: Credit[]) {
    return renderWithProviders(
      <dl>
        <CrewRows crew={crew} />
      </dl>,
    );
  }

  it("lists each job, singular or plural, linking to the people", () => {
    renderCrew([
      makeCredit(1, "Gene Kelly", "director"),
      makeCredit(2, "Stanley Donen", "director"),
      makeCredit(3, "Some Writer", "writer"),
      makeCredit(4, "A Producer", "producer"),
    ]);

    const terms = screen.getAllByRole("term").map((t) => t.textContent);
    expect(terms).toEqual(["Directors", "Writer"]);
    expect(screen.getByRole("link", { name: "Stanley Donen" })).toHaveAttribute("href", personUrl(2));
    expect(screen.queryByText("A Producer")).not.toBeInTheDocument();
  });

  it("shortens long lists", () => {
    renderCrew(Array.from({ length: 6 }, (_, i) => makeCredit(i + 1, `Writer ${i + 1}`, "writer")));
    const people = screen.getByRole("definition");
    expect(within(people).getAllByRole("link")).toHaveLength(4);
    expect(people).toHaveTextContent("Writer 1, Writer 2, Writer 3, Writer 4 and 2 more");
  });
});

describe("CastRow", () => {
  it("shows the cast with their characters", () => {
    renderWithProviders(
      <CastRow title="Cast" credits={[makeCredit(1, "Gene Kelly", "actor", "Don Lockwood"), makeCredit(2, "Debbie Reynolds", "actor")]} />,
    );
    expect(screen.getByRole("heading", { name: "Cast" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Gene Kelly/ })).toHaveTextContent("Don Lockwood");
    expect(screen.getByRole("link", { name: /Debbie Reynolds/ })).toHaveAttribute("href", personUrl(2));
  });

  it("renders nothing without a cast", () => {
    const { container } = renderWithProviders(<CastRow title="Cast" credits={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});
