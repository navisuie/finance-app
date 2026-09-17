import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { formatDate } from "../lib/money";
import type { Account, Category, CsvMapping, Detection, ImportPreview, ImportResult, PreviewRow } from "../lib/types";
import { CategorySelect } from "./CategorySelect";
import { Modal } from "./Modal";
import { Money } from "./privacy";
import { controlClass, Select, selectClass } from "./Select";
import { Button } from "./ui";

const MAX_BYTES = 10 * 1024 * 1024;
const MAX_ROWS_SHOWN = 500;

type Step = "choose" | "map" | "done";

function defaultSelected(r: PreviewRow) {
  return r.status === "new";
}

function fallbackMapping(): CsvMapping {
  return {
    skipRows: 0,
    headerRow: null,
    dateColumn: 0,
    dateFormat: "%m/%d/%Y",
    payeeColumn: 1,
    amount: { mode: "single", column: 2, negate: false },
    categoryColumn: null,
    notesColumn: null,
  };
}

export function ImportWizard({
  accounts,
  categories,
  defaultAccountId,
  onClose,
  onImported,
}: {
  accounts: Account[];
  categories: Category[];
  defaultAccountId?: number;
  onClose: () => void;
  onImported: () => void;
}) {
  const open = accounts.filter((a) => !a.archived);
  const [step, setStep] = useState<Step>("choose");
  const [accountId, setAccountId] = useState<number>(defaultAccountId ?? open[0]?.id ?? 0);
  const [fileName, setFileName] = useState("");
  const [text, setText] = useState("");
  const [detection, setDetection] = useState<Detection>();
  const [mapping, setMapping] = useState<CsvMapping>(fallbackMapping());
  const [preview, setPreview] = useState<ImportPreview>();
  const [previewError, setPreviewError] = useState<string>();
  const [toggled, setToggled] = useState<Set<number>>(new Set());
  const [overrides, setOverrides] = useState<Record<number, number | null>>({});
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ImportResult>();
  const [profileName, setProfileName] = useState("");
  const [profileMsg, setProfileMsg] = useState<string>();
  const profiles = useAsync(() => api.listImportProfiles(), []);

  const onFile = async (file: File | undefined) => {
    setError(undefined);
    if (!file) return;
    if (file.size > MAX_BYTES) {
      setError("That file is larger than 10 MB.");
      return;
    }
    setBusy(true);
    try {
      const content = await file.text();
      const d = await api.importDetect(accountId, content);
      setFileName(file.name);
      setText(content);
      setDetection(d);
      setMapping(d.mapping ?? fallbackMapping());
      setToggled(new Set());
      setOverrides({});
      setStep("map");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  // Live preview whenever the mapping changes.
  useEffect(() => {
    if (step !== "map" || !text) return;
    let cancelled = false;
    const t = setTimeout(() => {
      api
        .importPreview(accountId, text, mapping)
        .then((p) => {
          if (cancelled) return;
          setPreview(p);
          setPreviewError(undefined);
        })
        .catch((e) => {
          if (cancelled) return;
          setPreview(undefined);
          setPreviewError(errorMessage(e));
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [step, text, mapping, accountId]);

  const isSelected = (r: PreviewRow) => {
    if (r.status === "duplicate" || r.status === "error") return false;
    return defaultSelected(r) !== toggled.has(r.line);
  };

  const selectedLines = useMemo(
    () => (preview ? preview.rows.filter(isSelected).map((r) => r.line) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [preview, toggled],
  );

  const toggle = (line: number) =>
    setToggled((s) => {
      const n = new Set(s);
      if (n.has(line)) n.delete(line);
      else n.add(line);
      return n;
    });

  const commit = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const r = await api.importCommit(accountId, text, mapping, selectedLines, overrides);
      setResult(r);
      setStep("done");
      setText("");
      onImported();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const saveProfile = async () => {
    try {
      await api.saveImportProfile(profileName, mapping);
      setProfileMsg(`Saved "${profileName.trim()}".`);
      setProfileName("");
      profiles.reload();
    } catch (e) {
      setProfileMsg(errorMessage(e));
    }
  };

  const columnOptions = (detection?.columns ?? []).map((c, i) => ({ value: i, label: c }));
  const optionalColumns = [{ value: -1, label: "None" }, ...columnOptions];
  const set = (patch: Partial<CsvMapping>) => setMapping((m) => ({ ...m, ...patch }));

  if (open.length === 0) {
    return (
      <Modal title="Import CSV" onClose={onClose}>
        <p className="text-sm text-zinc-600 dark:text-zinc-400">Add an account first, then import its transactions.</p>
      </Modal>
    );
  }

  return (
    <Modal title="Import transactions from CSV" onClose={onClose} wide>
      {step === "choose" ? (
        <div className="space-y-4">
          <Select
            label="Import into account"
            value={accountId}
            onChange={(e) => setAccountId(Number(e.target.value))}
            options={open.map((a) => ({ value: a.id, label: a.name }))}
          />
          <label className="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-zinc-300 p-10 text-center hover:border-brand-500 dark:border-zinc-700">
            <span className="font-medium">{busy ? "Reading file…" : "Choose a CSV file"}</span>
            <span className="text-xs text-zinc-500">
              Download it from your bank's website (usually under Statements or Activity → Export). Up to 10 MB.
            </span>
            <input type="file" accept=".csv,text/csv" className="sr-only" disabled={busy} onChange={(e) => onFile(e.target.files?.[0])} />
          </label>
          <p className="text-xs text-zinc-500">The file is read on this Mac only. It is not uploaded anywhere.</p>
          {error ? <p className="text-sm text-red-600">{error}</p> : null}
        </div>
      ) : null}

      {step === "map" && detection ? (
        <div className="space-y-5">
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <span className="font-medium">{fileName}</span>
            <span className="text-zinc-500">· {detection.dataRowCount} rows</span>
            {detection.fromPreviousImport ? (
              <span className="rounded bg-brand-50 px-2 py-0.5 text-xs text-brand-700 dark:bg-brand-900/40 dark:text-brand-500">
                Using the settings from your last import into this account
              </span>
            ) : detection.mapping ? (
              <span className="rounded bg-zinc-100 px-2 py-0.5 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
                Columns detected automatically. Check them below.
              </span>
            ) : (
              <span className="rounded bg-amber-50 px-2 py-0.5 text-xs text-amber-800 dark:bg-amber-950 dark:text-amber-200">
                Couldn't detect the columns. Pick them below.
              </span>
            )}
            <div className="flex-1" />
            {profiles.data && profiles.data.length > 0 ? (
              <select
                className={`${controlClass} w-auto`}
                value=""
                onChange={(e) => {
                  const p = profiles.data?.find((x) => x.id === Number(e.target.value));
                  if (p) setMapping(p.mapping);
                }}
                aria-label="Load saved profile"
              >
                <option value="">Load saved profile…</option>
                {profiles.data.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            ) : null}
          </div>

          <div className="grid grid-cols-4 gap-3 rounded-xl bg-zinc-50 p-4 dark:bg-zinc-800/40">
            <Select label="Date column" value={mapping.dateColumn} onChange={(e) => set({ dateColumn: Number(e.target.value) })} options={columnOptions} />
            <Select
              label="Date format"
              value={mapping.dateFormat}
              onChange={(e) => set({ dateFormat: e.target.value })}
              options={detection.dateFormats.map((f) => ({ value: f.format, label: f.example }))}
            />
            <Select label="Description column" value={mapping.payeeColumn} onChange={(e) => set({ payeeColumn: Number(e.target.value) })} options={columnOptions} />
            <Select
              label="Amount layout"
              value={mapping.amount.mode}
              onChange={(e) =>
                set({
                  amount:
                    e.target.value === "single"
                      ? { mode: "single", column: 0, negate: false }
                      : { mode: "split", debitColumn: 0, creditColumn: 0 },
                })
              }
              options={[
                { value: "single", label: "One amount column" },
                { value: "split", label: "Separate debit / credit" },
              ]}
            />
            {mapping.amount.mode === "single" ? (
              <>
                <Select
                  label="Amount column"
                  value={mapping.amount.column}
                  onChange={(e) => set({ amount: { mode: "single", column: Number(e.target.value), negate: mapping.amount.mode === "single" && mapping.amount.negate } })}
                  options={columnOptions}
                />
                <label className="flex items-end gap-2 pb-2 text-sm">
                  <input
                    type="checkbox"
                    checked={mapping.amount.negate}
                    onChange={(e) => mapping.amount.mode === "single" && set({ amount: { ...mapping.amount, negate: e.target.checked } })}
                  />
                  <span>Purchases are positive in this file (flip signs)</span>
                </label>
              </>
            ) : (
              <>
                <Select
                  label="Money out column"
                  value={mapping.amount.debitColumn}
                  onChange={(e) => mapping.amount.mode === "split" && set({ amount: { ...mapping.amount, debitColumn: Number(e.target.value) } })}
                  options={columnOptions}
                />
                <Select
                  label="Money in column"
                  value={mapping.amount.creditColumn}
                  onChange={(e) => mapping.amount.mode === "split" && set({ amount: { ...mapping.amount, creditColumn: Number(e.target.value) } })}
                  options={columnOptions}
                />
              </>
            )}
            <Select
              label="Category column"
              value={mapping.categoryColumn ?? -1}
              onChange={(e) => set({ categoryColumn: Number(e.target.value) < 0 ? null : Number(e.target.value) })}
              options={optionalColumns}
            />
            <Select
              label="Notes column"
              value={mapping.notesColumn ?? -1}
              onChange={(e) => set({ notesColumn: Number(e.target.value) < 0 ? null : Number(e.target.value) })}
              options={optionalColumns}
            />
            <div className="space-y-1.5">
              <label className="block text-sm font-medium text-zinc-700 dark:text-zinc-300" htmlFor="skip-rows">
                Rows to skip at top
              </label>
              <input
                id="skip-rows"
                type="number"
                min={0}
                max={50}
                value={mapping.skipRows}
                onChange={(e) => set({ skipRows: Math.max(0, Math.min(50, Number(e.target.value) || 0)) })}
                className={selectClass}
              />
            </div>
          </div>

          {previewError ? <p className="text-sm text-red-600">{previewError}</p> : null}

          {preview ? (
            <>
              <div className="flex flex-wrap gap-2 text-xs">
                <Badge tone="green">{preview.newCount} new</Badge>
                {preview.possibleDuplicateCount > 0 ? <Badge tone="amber">{preview.possibleDuplicateCount} possible duplicates (unchecked)</Badge> : null}
                {preview.duplicateCount > 0 ? <Badge tone="zinc">{preview.duplicateCount} already imported</Badge> : null}
                {preview.errorCount > 0 ? <Badge tone="red">{preview.errorCount} can't be read</Badge> : null}
                {preview.firstDate && preview.lastDate ? (
                  <span className="text-zinc-500">
                    {formatDate(preview.firstDate)} – {formatDate(preview.lastDate)}
                  </span>
                ) : null}
              </div>
              <div className="max-h-[45vh] overflow-auto rounded-xl ring-1 ring-zinc-200 dark:ring-zinc-800">
                <table className="w-full text-sm">
                  <thead className="sticky top-0 bg-zinc-50 text-left text-xs text-zinc-500 dark:bg-zinc-800">
                    <tr>
                      <th className="w-8 px-3 py-2" />
                      <th className="px-3 py-2 font-medium">Line</th>
                      <th className="px-3 py-2 font-medium">Date</th>
                      <th className="px-3 py-2 font-medium">Description</th>
                      <th className="px-3 py-2 font-medium">Category</th>
                      <th className="px-3 py-2 text-right font-medium">Amount</th>
                      <th className="px-3 py-2 font-medium">Status</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
                    {preview.rows.slice(0, MAX_ROWS_SHOWN).map((r) => {
                      const disabled = r.status === "duplicate" || r.status === "error";
                      return (
                        <tr key={r.line} className={disabled ? "text-zinc-400" : ""}>
                          <td className="px-3 py-1.5">
                            <input type="checkbox" checked={isSelected(r)} disabled={disabled} onChange={() => toggle(r.line)} aria-label={`Import line ${r.line}`} />
                          </td>
                          <td className="px-3 py-1.5 text-zinc-400">{r.line}</td>
                          <td className="px-3 py-1.5 whitespace-nowrap">{r.postedOn ? formatDate(r.postedOn) : "—"}</td>
                          <td className="px-3 py-1.5">{r.payee || "—"}</td>
                          <td className="px-3 py-1">
                            {disabled ? null : (
                              <CategorySelect
                                categories={categories}
                                value={r.line in overrides ? overrides[r.line] : r.categoryId}
                                onChange={(cid) => setOverrides((o) => ({ ...o, [r.line]: cid }))}
                                className="w-36 rounded-md border-0 bg-transparent px-1 py-0.5 text-sm hover:ring-1 hover:ring-zinc-300"
                                aria-label={`Category for line ${r.line}`}
                              />
                            )}
                          </td>
                          <td className="px-3 py-1.5 text-right whitespace-nowrap">
                            {r.amountCents !== null ? <Money cents={r.amountCents} colored /> : "—"}
                          </td>
                          <td className="px-3 py-1.5 text-xs">
                            <StatusLabel row={r} />
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
              {preview.rows.length > MAX_ROWS_SHOWN ? (
                <p className="text-xs text-zinc-500">
                  Showing the first {MAX_ROWS_SHOWN} rows. All {preview.newCount} new rows will be imported.
                </p>
              ) : null}
            </>
          ) : null}

          <div className="flex flex-wrap items-center gap-2 border-t border-zinc-200 pt-4 dark:border-zinc-800">
            <input
              value={profileName}
              onChange={(e) => setProfileName(e.target.value)}
              placeholder="Profile name, e.g. My bank"
              maxLength={60}
              className={`${controlClass} w-52`}
              aria-label="Profile name"
            />
            <Button variant="secondary" onClick={saveProfile} disabled={!profileName.trim()}>
              Save column settings
            </Button>
            {profileMsg ? <span className="text-xs text-zinc-500">{profileMsg}</span> : null}
            <div className="flex-1" />
            {error ? <span className="text-sm text-red-600">{error}</span> : null}
            <Button variant="secondary" onClick={() => setStep("choose")}>
              Back
            </Button>
            <Button onClick={commit} disabled={busy || selectedLines.length === 0}>
              {busy ? "Importing…" : `Import ${selectedLines.length} transaction${selectedLines.length === 1 ? "" : "s"}`}
            </Button>
          </div>
        </div>
      ) : null}

      {step === "done" && result ? (
        <div className="space-y-4">
          <p className="text-lg font-medium">
            Imported {result.inserted} transaction{result.inserted === 1 ? "" : "s"}.
          </p>
          {result.skipped > 0 ? <p className="text-sm text-zinc-500">{result.skipped} rows were skipped.</p> : null}
          <p className="text-sm text-zinc-500">
            Next time you import into this account, the same column settings are used automatically. Remember to update the
            account balance if it changed.
          </p>
          <div className="flex justify-end">
            <Button onClick={onClose}>Done</Button>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}

function Badge({ tone, children }: { tone: "green" | "amber" | "zinc" | "red"; children: ReactNode }) {
  const tones = {
    green: "bg-brand-50 text-brand-700 dark:bg-brand-900/40 dark:text-brand-500",
    amber: "bg-amber-50 text-amber-800 dark:bg-amber-950 dark:text-amber-200",
    zinc: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300",
    red: "bg-red-50 text-red-700 dark:bg-red-950 dark:text-red-300",
  };
  return <span className={`rounded-full px-2.5 py-1 font-medium ${tones[tone]}`}>{children}</span>;
}

function StatusLabel({ row }: { row: PreviewRow }) {
  switch (row.status) {
    case "new":
      return <span className="text-brand-700 dark:text-brand-500">New</span>;
    case "duplicate":
      return <span>Already imported</span>;
    case "possible_duplicate":
      return (
        <span className="text-amber-700 dark:text-amber-300" title={row.message ?? undefined}>
          Possible duplicate
        </span>
      );
    case "error":
      return <span className="text-red-600">{row.message}</span>;
  }
}
