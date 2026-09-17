import { useEffect } from "react";
import { api } from "../lib/api";

const HEARTBEAT_MS = 30_000;
const EVENTS = ["mousemove", "mousedown", "keydown", "wheel", "touchstart"] as const;

/**
 * Tells the backend the user is active, at most once every 30 s. The backend
 * owns the idle timer and locks the vault on its own when it expires.
 */
export function useActivityHeartbeat(enabled: boolean, onLocked: () => void) {
  useEffect(() => {
    if (!enabled) return;
    let last = 0;
    const ping = () => {
      const now = Date.now();
      if (now - last < HEARTBEAT_MS) return;
      last = now;
      api.activity().catch(() => onLocked());
    };
    ping();
    EVENTS.forEach((e) => window.addEventListener(e, ping, { passive: true }));
    return () => EVENTS.forEach((e) => window.removeEventListener(e, ping));
  }, [enabled, onLocked]);
}
