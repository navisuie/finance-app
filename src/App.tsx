import { useCallback, useEffect, useState } from "react";
import { api, onVaultLocked } from "./lib/api";
import { useActivityHeartbeat } from "./hooks/useActivityHeartbeat";
import { SetupScreen } from "./pages/SetupScreen";
import { LockScreen } from "./pages/LockScreen";
import { RecoverScreen } from "./pages/RecoverScreen";
import { AppShell } from "./pages/AppShell";

type View = "loading" | "setup" | "locked" | "recover" | "unlocked";

export default function App() {
  const [view, setView] = useState<View>("loading");
  const [notice, setNotice] = useState<string>();

  const refresh = useCallback(async () => {
    const s = await api.vaultStatus();
    setView(!s.exists ? "setup" : s.unlocked ? "unlocked" : "locked");
  }, []);

  useEffect(() => {
    refresh().catch(() => setView("locked"));
    const unlisten = onVaultLocked((reason) => {
      setNotice(reason === "idle" ? "Locked after inactivity." : undefined);
      setView((v) => (v === "unlocked" ? "locked" : v));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [refresh]);

  const handleLocked = useCallback(() => {
    setView((v) => (v === "unlocked" ? "locked" : v));
  }, []);

  useActivityHeartbeat(view === "unlocked", handleLocked);

  switch (view) {
    case "loading":
      return null;
    case "setup":
      return <SetupScreen onReady={() => setView("unlocked")} />;
    case "recover":
      return <RecoverScreen onDone={() => setView("unlocked")} onCancel={() => setView("locked")} />;
    case "locked":
      return (
        <LockScreen
          notice={notice}
          onUnlocked={() => {
            setNotice(undefined);
            setView("unlocked");
          }}
          onForgot={() => setView("recover")}
        />
      );
    case "unlocked":
      return (
        <AppShell
          key="shell"
          onLock={() => {
            api.lock().finally(() => {
              setNotice(undefined);
              setView("locked");
            });
          }}
        />
      );
  }
}
