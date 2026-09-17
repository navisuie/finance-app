import { useState } from "react";
import { Button } from "./ui";

/**
 * Shows a recovery key once and requires the user to confirm they saved it.
 */
export function RecoveryKeyPanel({ recoveryKey, onDone }: { recoveryKey: string; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(recoveryKey);
      setCopied(true);
      // Best effort: clear the clipboard after 60 s if it still holds the key.
      setTimeout(async () => {
        try {
          if ((await navigator.clipboard.readText()) === recoveryKey) {
            await navigator.clipboard.writeText("");
          }
        } catch {
          /* clipboard read may be denied; ignore */
        }
      }, 60_000);
    } catch {
      setCopied(false);
    }
  };

  const groups = recoveryKey.split("-");

  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-xl font-semibold">Save your recovery key</h1>
        <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
          This is the only way back in if you forget your password. It is shown once. Write it on paper and store it
          somewhere safe, or save it in a password manager.
        </p>
      </div>
      <div className="selectable grid grid-cols-4 gap-2 rounded-lg bg-zinc-100 p-4 font-mono text-sm tracking-wider dark:bg-zinc-800">
        {groups.map((g, i) => (
          <span key={i} className="text-center">
            {g}
          </span>
        ))}
      </div>
      <div className="flex gap-2">
        <Button variant="secondary" onClick={copy} type="button">
          {copied ? "Copied (clears in 60 s)" : "Copy"}
        </Button>
      </div>
      <label className="flex items-start gap-2 text-sm">
        <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} className="mt-0.5" />
        <span>I've stored my recovery key somewhere safe. I understand my data can't be recovered without it.</span>
      </label>
      <Button className="w-full" disabled={!saved} onClick={onDone}>
        Continue
      </Button>
    </div>
  );
}
