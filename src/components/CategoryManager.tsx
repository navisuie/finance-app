import { useState, type FormEvent } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import type { Category, CategoryKind } from "../lib/types";
import { useConfirm } from "./Confirm";
import { controlClass, selectClass } from "./Select";
import { Button, Card } from "./ui";

const KINDS: { value: CategoryKind; label: string }[] = [
  { value: "expense", label: "Spending" },
  { value: "income", label: "Income" },
  { value: "transfer", label: "Transfer" },
];

export function CategoryManager() {
  const cats = useAsync(() => api.listCategories(), []);
  const [name, setName] = useState("");
  const [kind, setKind] = useState<CategoryKind>("expense");
  const [msg, setMsg] = useState<string>();
  const [editId, setEditId] = useState<number>();
  const [editName, setEditName] = useState("");
  const [confirm, dialog] = useConfirm();

  const add = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.createCategory({ name, kind });
      setName("");
      setMsg(undefined);
      cats.reload();
    } catch (err) {
      setMsg(errorMessage(err));
    }
  };

  const rename = async (c: Category) => {
    try {
      await api.updateCategory(c.id, { name: editName, kind: c.kind });
      setEditId(undefined);
      setMsg(undefined);
      cats.reload();
    } catch (err) {
      setMsg(errorMessage(err));
    }
  };

  const changeKind = async (c: Category, k: CategoryKind) => {
    try {
      await api.updateCategory(c.id, { name: c.name, kind: k });
      cats.reload();
    } catch (err) {
      setMsg(errorMessage(err));
    }
  };

  const remove = async (c: Category) => {
    const ok = await confirm({
      title: `Delete "${c.name}"?`,
      body:
        c.transactionCount > 0
          ? `${c.transactionCount} transaction(s) will become uncategorized.`
          : "No transactions use this category.",
    });
    if (!ok) return;
    try {
      await api.deleteCategory(c.id);
      cats.reload();
    } catch (err) {
      setMsg(errorMessage(err));
    }
  };

  return (
    <Card title="Categories">
      <p className="mb-3 text-xs text-zinc-500">
        Transfers (like moving money to your HYSA or paying a credit card) are left out of spending and income.
      </p>
      <ul className="max-h-80 divide-y divide-zinc-100 overflow-y-auto text-sm dark:divide-zinc-800">
        {(cats.data ?? []).map((c) => (
          <li key={c.id} className="flex items-center gap-2 py-1.5">
            {editId === c.id ? (
              <form
                className="flex flex-1 gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  rename(c);
                }}
              >
                <input value={editName} onChange={(e) => setEditName(e.target.value)} className={selectClass} autoFocus maxLength={60} />
                <Button type="submit" className="py-1">
                  Save
                </Button>
                <Button type="button" variant="ghost" className="py-1" onClick={() => setEditId(undefined)}>
                  Cancel
                </Button>
              </form>
            ) : (
              <>
                <button
                  className="flex-1 text-left hover:underline"
                  onClick={() => {
                    setEditId(c.id);
                    setEditName(c.name);
                  }}
                >
                  {c.name}
                </button>
                <span className="text-xs text-zinc-400">{c.transactionCount}</span>
                <select
                  value={c.kind}
                  onChange={(e) => changeKind(c, e.target.value as CategoryKind)}
                  className="rounded-md bg-transparent px-1 py-0.5 text-xs text-zinc-500"
                  aria-label={`Type of ${c.name}`}
                >
                  {KINDS.map((k) => (
                    <option key={k.value} value={k.value}>
                      {k.label}
                    </option>
                  ))}
                </select>
                <button onClick={() => remove(c)} className="text-xs text-zinc-400 hover:text-red-600" aria-label={`Delete ${c.name}`}>
                  ✕
                </button>
              </>
            )}
          </li>
        ))}
      </ul>
      <form onSubmit={add} className="mt-3 flex gap-2">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="New category"
          maxLength={60}
          className={selectClass}
          aria-label="New category name"
        />
        <select value={kind} onChange={(e) => setKind(e.target.value as CategoryKind)} className={`${controlClass} w-36`} aria-label="New category type">
          {KINDS.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
        <Button type="submit" disabled={!name.trim()}>
          Add
        </Button>
      </form>
      {msg ? <p className="mt-2 text-sm text-red-600">{msg}</p> : null}
      {dialog}
    </Card>
  );
}
