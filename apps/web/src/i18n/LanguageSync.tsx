import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useMe, useServer } from "@/api/hooks";
import { currentLanguage, pickLanguage, setLanguage } from ".";

/**
 * Keeps the app in the language it should be in: the signed-in account's,
 * else the server's, else the browser's. Reads the account from the cache
 * without asking for it (pages that need it do).
 */
export function LanguageSync() {
  const server = useServer();
  const me = useMe(false);
  const qc = useQueryClient();
  // Until the server (and an account being asked for) has answered, the
  // language of the last visit stays: going by the browser's meanwhile
  // would flash it.
  const waiting = server.isPending || (me.isPending && me.fetchStatus === "fetching");
  const wanted = waiting ? null : pickLanguage(me.data?.language, server.data?.language);

  useEffect(() => {
    if (!wanted || wanted === currentLanguage()) return;
    void setLanguage(wanted).then(
      // The server writes some words itself (Home's rows): asked again, in the new language.
      () => qc.invalidateQueries({ predicate: (query) => query.queryKey[0] !== "me" && query.queryKey[0] !== "server" }),
      () => undefined,
    );
  }, [wanted, qc]);

  return null;
}
