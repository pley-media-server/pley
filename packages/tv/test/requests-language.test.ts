import { afterEach, describe, expect, it } from "vitest";
import type { MediaRequest } from "../src/api/types";
import { setLanguage } from "../src/i18n";
import { actionRows, allowanceLines, askedText, newsText, pickerButton, readyText, requestsEmpty, requestsTabs, seasonsLabel, stageWords } from "../src/requests";

/** French's no-break space, before : ; ? ! and inside « ». */
const NB = " ";

function request(over: Partial<MediaRequest> = {}): MediaRequest {
  return {
    id: 1,
    kind: "movie",
    tmdb_id: 10,
    title: "Arrival",
    year: 2016,
    poster_url: null,
    backdrop_url: null,
    seasons: null,
    available_seasons: [],
    anime: false,
    status: "pending",
    stage: "pending",
    stage_at: null,
    reason: null,
    note: null,
    mine: true,
    watching: false,
    watchers: 0,
    unseen: false,
    actions: [],
    requested_by: null,
    decided_by: null,
    decided_at: null,
    created_at: 0,
    updated_at: 0,
    retry_at: null,
    download: null,
    item_id: null,
    ...over,
  } as MediaRequest;
}

const limits = { movies_next_at: null, seasons_next_at: null };

afterEach(() => setLanguage("en"));

describe("requests in French and Spanish", () => {
  it("name seasons, and what of a show is ready", () => {
    setLanguage("fr");
    expect(seasonsLabel([0, 1, 2, 3, 5])).toBe("Saisons 1–3, 5 et épisodes spéciaux");
    expect(seasonsLabel([2])).toBe("Saison 2");
    expect(readyText("Severance", [2], false)).toBe(`Saison 2 de «${NB}Severance${NB}» est prête à regarder.`);
    expect(readyText("Severance", [0], false)).toBe(`Les épisodes spéciaux de «${NB}Severance${NB}» sont prêts à regarder.`);
    setLanguage("es");
    expect(readyText("Severance", [1, 2], false)).toBe("Temporadas 1–2 de «Severance» ya están listas para ver.");
    expect(readyText("Arrival", null, true)).toBe("«Arrival» está listo para ver.");
  });

  it("count as each language does", () => {
    expect(newsText(1)).toBe("1 of your requests has news");
    expect(newsText(3)).toBe("3 of your requests have news");
    setLanguage("fr");
    expect(newsText(1)).toBe("1 de vos demandes a du nouveau");
    expect(pickerButton(2)).toBe("Demander 2 saisons");
    expect(allowanceLines({ movies_left: 1, seasons_left: 3, movies_limit: 5, seasons_limit: 1, ...limits })).toEqual([
      "Vous pouvez encore demander 1 film et 3 saisons.",
      `Limites${NB}: 5 films et 1 saison sur 7 jours.`,
    ]);
    setLanguage("es");
    expect(pickerButton(1)).toBe("Solicitar 1 temporada");
    expect(allowanceLines({ movies_left: 2, seasons_left: null, movies_limit: 4, ...limits, kind: "movie" })).toEqual([
      "Puedes solicitar 2 películas más.",
      "Límites: 4 películas cada 7 días.",
    ]);
  });

  it("say where a request has got to and what may be done with it", () => {
    setLanguage("fr");
    expect(stageWords(request(), { manage: false, hours24: true }).text).toBe("En attente d’approbation");
    expect(stageWords(request({ stage: "declined", status: "declined", reason: "Already on disc" }), { manage: false, hours24: true }).note).toBe(
      `«${NB}Already on disc${NB}»`,
    );
    const withdraw = actionRows(request({ actions: ["withdraw"] }))[0]!;
    expect(withdraw.label).toBe("Annuler la demande");
    expect(withdraw.confirm?.title).toBe(`Annuler votre demande pour «${NB}Arrival${NB}»${NB}?`);
    expect(askedText(request(), true)).toBe(`Demande envoyée pour «${NB}Arrival${NB}»${NB}: un administrateur décidera.`);
    setLanguage("es");
    expect(requestsTabs(true).map((tab) => tab.label)).toEqual(["Por revisar", "Todas", "En espera", "Aprobadas", "Listas", "Rechazadas", "Fallidas"]);
    expect(requestsEmpty("pending", false)).toBe("Ninguna de tus solicitudes espera una decisión.");
    expect(actionRows(request({ actions: ["delete"] }))[0]!.confirm?.title).toBe("¿Eliminar esta solicitud?");
  });
});
