import { useState, type FormEvent } from "react";
import { useConfirm } from "../components/Confirm";
import { Meter } from "../components/Meter";
import { Modal } from "../components/Modal";
import { Money } from "../components/privacy";
import { Select } from "../components/Select";
import { Button, Card, Field } from "../components/ui";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { centsToInput, formatDate, parseMoney } from "../lib/money";
import type { Account, Goal } from "../lib/types";

export function GoalsPage() {
  const goals = useAsync(() => api.listGoals(), []);
  const accounts = useAsync(() => api.listAccounts(false), []);
  const [editing, setEditing] = useState<Goal | "new">();
  const [confirm, dialog] = useConfirm();
  const [error, setError] = useState<string>();

  const savings = (accounts.data ?? []).filter((a) => a.isAsset);

  const remove = async (goal: Goal) => {
    const ok = await confirm({
      title: `Delete "${goal.name}"?`,
      body: "The goal is removed. The money and the account it tracks stay exactly as they are.",
    });
    if (!ok) return;
    try {
      await api.deleteGoal(goal.id);
      goals.reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="max-w-4xl space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Goals</h1>
        <Button onClick={() => setEditing("new")}>Add goal</Button>
      </div>
      {error ? <p className="text-sm text-red-600">{error}</p> : null}

      {goals.data && goals.data.length === 0 ? (
        <Card>
          <p className="font-medium">No goals yet</p>
          <p className="mt-1 text-sm text-zinc-500">
            Set a target like a house down payment or an emergency fund, point it at the account where the money sits,
            and this tells you what to put aside each month.
          </p>
          <Button className="mt-4" onClick={() => setEditing("new")}>
            Add your first goal
          </Button>
        </Card>
      ) : null}

      <div className="grid grid-cols-2 gap-4">
        {(goals.data ?? []).map((g) => (
          <GoalCard key={g.id} goal={g} onEdit={() => setEditing(g)} onDelete={() => remove(g)} />
        ))}
      </div>

      {editing ? (
        <GoalForm
          goal={editing === "new" ? undefined : editing}
          accounts={savings}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            goals.reload();
          }}
        />
      ) : null}
      {dialog}
    </div>
  );
}

function GoalCard({ goal, onEdit, onDelete }: { goal: Goal; onEdit: () => void; onDelete: () => void }) {
  return (
    <Card>
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="font-medium">{goal.name}</p>
          <p className="text-xs text-zinc-500">
            {goal.accountName ?? "No account linked"}
            {goal.targetDate ? ` · by ${formatDate(goal.targetDate)}` : ""}
          </p>
        </div>
        <div className="flex gap-1">
          <Button variant="ghost" className="py-1 text-xs" onClick={onEdit}>
            Edit
          </Button>
          <Button variant="ghost" className="py-1 text-xs text-red-600" onClick={onDelete}>
            Delete
          </Button>
        </div>
      </div>

      <p className="mt-3 text-2xl font-semibold">
        <Money cents={goal.currentCents} />{" "}
        <span className="text-base font-normal text-zinc-500">
          of <Money cents={goal.targetCents} />
        </span>
      </p>
      <div className="mt-2">
        <Meter used={goal.progress} label={`${goal.name} progress`} />
      </div>
      <p className="mt-2 text-sm">
        {goal.reached ? (
          <span className="text-brand-700 dark:text-brand-500">Target reached.</span>
        ) : goal.overdue ? (
          <span className="text-amber-700 dark:text-amber-400">
            Past the target date · <Money cents={goal.remainingCents} /> to go
          </span>
        ) : goal.monthlyNeededCents !== null ? (
          <>
            <Money cents={goal.monthlyNeededCents} />
            <span className="text-zinc-500">
              {" "}
              a month for {goal.monthsLeft} month{goal.monthsLeft === 1 ? "" : "s"}
            </span>
          </>
        ) : (
          <>
            <Money cents={goal.remainingCents} /> <span className="text-zinc-500">to go</span>
          </>
        )}
      </p>
      {!goal.accountName ? (
        <p className="mt-1 text-xs text-zinc-500">Link an account and progress follows its balance.</p>
      ) : null}
    </Card>
  );
}

function GoalForm({
  goal,
  accounts,
  onClose,
  onSaved,
}: {
  goal?: Goal;
  accounts: Account[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(goal?.name ?? "");
  const [target, setTarget] = useState(goal ? centsToInput(goal.targetCents) : "");
  const [date, setDate] = useState(goal?.targetDate ?? "");
  const [accountId, setAccountId] = useState<number | null>(goal?.accountId ?? accounts[0]?.id ?? null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const targetCents = parseMoney(target);
    if (targetCents === null || targetCents <= 0) {
      setError("Enter the amount you're aiming for, like 20000.");
      return;
    }
    setBusy(true);
    try {
      const input = { name, targetCents, targetDate: date || null, accountId };
      if (goal) await api.updateGoal(goal.id, input);
      else await api.createGoal(input);
      onSaved();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={goal ? "Edit goal" : "Add goal"} onClose={onClose}>
      <form onSubmit={submit} className="space-y-4">
        <Field
          label="Goal"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="House down payment"
          maxLength={80}
          required
        />
        <div className="grid grid-cols-2 gap-3">
          <Field
            label="Target amount"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            inputMode="decimal"
            placeholder="60000.00"
            required
          />
          <Field label="Target date (optional)" type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </div>
        <Select
          label="Money is held in"
          value={accountId ?? ""}
          onChange={(e) => setAccountId(e.target.value ? Number(e.target.value) : null)}
          options={[
            { value: "", label: "No account yet" },
            ...accounts.map((a) => ({ value: a.id, label: a.name })),
          ]}
        />
        <p className="text-xs text-zinc-500">
          Progress follows that account's balance, so it updates when the balance does. Pick a date and the goal works
          out what to save each month.
        </p>
        {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !name.trim() || !target.trim()}>
            {goal ? "Save" : "Add goal"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
