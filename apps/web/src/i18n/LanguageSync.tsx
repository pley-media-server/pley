import { useEffect, useSyncExternalStore } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useMe, useServer } from "@/api/hooks";
import { currentLanguage, deviceChoiceLanguage, deviceLanguage, onDeviceLanguage, pickLanguage, setLanguage } from ".";

/**
 * Keeps the app in the language it should be in: the signed-in account's,
 * else this device's, else the server language. Reads the account from the
 * cache without asking for it (pages that need it do).
 */
export function LanguageSync() {
  const server = useServer();
  const me = useMe(false);
  const qc = useQueryClient();
  const deviceChoice = useSyncExternalStore(onDeviceLanguage, deviceChoiceLanguage);
  // Until an account being asked for has answered (and the server, where
  // this device has no language Pley has), the language of the last visit
  // stays: going by another meanwhile would flash it.
  const deviceSays = deviceChoice ?? deviceLanguage();
  const waiting = (me.isPending && me.fetchStatus === "fetching") || (!deviceSays && server.isPending);
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
