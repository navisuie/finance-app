import { useState } from "react";
import { ImportWizard } from "../components/ImportWizard";
import { TransactionList } from "../components/TransactionList";
import { Button } from "../components/ui";
import { useAsync } from "../hooks/useAsync";
import { api } from "../lib/api";

export function TransactionsPage() {
  const accounts = useAsync(() => api.listAccounts(true), []);
  const categories = useAsync(() => api.listCategories(), []);
  const [importOpen, setImportOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  return (
    <div className="max-w-6xl space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Transactions</h1>
        <Button variant="secondary" onClick={() => setImportOpen(true)} disabled={!accounts.data?.some((a) => !a.archived)}>
          Import CSV
        </Button>
      </div>
      {accounts.error || categories.error ? <p className="text-sm text-red-600">{accounts.error ?? categories.error}</p> : null}
      {accounts.data && accounts.data.length === 0 ? (
        <p className="text-sm text-zinc-500">Add an account on the Accounts page to start recording transactions.</p>
      ) : null}
      {accounts.data && categories.data ? (
        <TransactionList
          accounts={accounts.data}
          categories={categories.data}
          refreshKey={refreshKey}
          onChanged={() => categories.reload()}
        />
      ) : null}
      {importOpen && accounts.data && categories.data ? (
        <ImportWizard
          accounts={accounts.data}
          categories={categories.data}
          onClose={() => setImportOpen(false)}
          onImported={() => setRefreshKey((k) => k + 1)}
        />
      ) : null}
    </div>
  );
}
