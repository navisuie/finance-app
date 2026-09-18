import { useState } from "react";
import { useAsync } from "../../hooks/useAsync";
import { api, errorMessage } from "../../lib/api";
import { Button } from "../ui";
import { useConfirm } from "../Confirm";

/** Save/load/delete named scenarios for one calculator. Generic over the
 * input shape — the core stores it as opaque JSON. */
export function ScenarioBar<T>({ calculator, input, onLoad }: { calculator: string; input: T; onLoad: (input: T) => void }) {
  const scenarios = useAsync(() => api.listScenarios(calculator), [calculator]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [confirm, confirmDialog] = useConfirm();

  const save = async () => {
    if (!name.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      await api.saveScenario(calculator, name.trim(), input);
      setName("");
      scenarios.reload();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: number) => {
    const ok = await confirm({ title: "Delete this scenario?", body: "This can't be undone." });
    if (!ok) return;
    try {
      await api.deleteScenario(id);
      scenarios.reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2 border-t border-zinc-100 pt-3 text-sm dark:border-zinc-800">
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Scenario name"
        className="w-40 rounded-lg border-0 bg-white px-2.5 py-1.5 text-sm ring-1 ring-zinc-300 outline-none focus:ring-2 focus:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-700"
      />
      <Button variant="secondary" className="py-1" disabled={!name.trim() || busy} onClick={save}>
        Save scenario
      </Button>
      {error ? <span className="text-red-600">{error}</span> : null}
      {scenarios.data && scenarios.data.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1.5">
          {scenarios.data.map((s) => (
            <span key={s.id} className="inline-flex items-center gap-1 rounded-full bg-zinc-100 pl-2.5 pr-1 py-0.5 dark:bg-zinc-800">
              <button className="hover:underline" onClick={() => onLoad(s.inputs as T)}>
                {s.name}
              </button>
              <button
                className="rounded-full px-1 text-zinc-400 hover:bg-zinc-200 hover:text-zinc-700 dark:hover:bg-zinc-700"
                aria-label={`Delete ${s.name}`}
                onClick={() => remove(s.id)}
              >
                ✕
              </button>
            </span>
          ))}
        </div>
      ) : null}
      {confirmDialog}
    </div>
  );
}
