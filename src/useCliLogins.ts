import { useEffect, useState } from "react";
import { bridge, errorMessage } from "./bridge";
import type { LoginStatus, Provider } from "./domain";

const POLL_INTERVAL_MS = 2000;

/** Tracks in-app CLI sign-ins per connection and polls the one in progress. */
export function useCliLogins(
  connectionId: string,
  onError: (message: string) => void,
) {
  const [logins, setLogins] = useState<Partial<Record<string, LoginStatus>>>(
    {},
  );
  const login = logins[connectionId];
  useEffect(() => {
    if (login?.state !== "waiting") return;
    let active = true;
    let pending = false;
    const timer = window.setInterval(() => {
      if (pending) return;
      pending = true;
      void bridge
        .loginStatus(login.provider, login.connectionId)
        .then((value) => {
          if (active)
            setLogins((previous) => ({
              ...previous,
              [value.connectionId]: value,
            }));
        })
        .catch((cause: unknown) => {
          if (active) onError(errorMessage(cause));
        })
        .finally(() => {
          pending = false;
        });
    }, POLL_INTERVAL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
    // onError is a stable state setter supplied by the caller.
  }, [login?.state, login?.provider, login?.connectionId]);
  const begin = async (
    provider: Provider,
    id: string,
    profile: string | null,
  ) => {
    const status = await bridge.login(provider, id, profile);
    setLogins((previous) => ({ ...previous, [id]: status }));
  };
  const forget = (id: string) =>
    setLogins((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([key]) => key !== id),
      ),
    );
  return { login, begin, forget };
}
