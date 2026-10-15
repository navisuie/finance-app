import { useState } from "react";
import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { formatDate } from "../lib/money";
import { CategorySelect } from "./CategorySelect";
import { Modal } from "./Modal";
import { Money } from "./privacy";

const LIMIT = 500;

/**
 * The transactions a budget line is counting for one month — the same rules
 * the budget uses (tagged transfers left out, refunds subtracted), so the
 * total at the bottom is the "spent" figure on the bar. Anything in the
 * wrong category can be moved from here, and the budget follows.
 */
export function CategoryTransactions({
  categoryId,
  categoryName,
  month,
  monthLabel,
  onClose,
  onChanged,
}: {
  categoryId: number;
  categoryName: string;
  /** YYYY-MM */
  month: string;
  monthLabel: string;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [y, m] = month.split("-").map(Number);
  const from = `${month}-01`;
  const to = `${month}-${String(new Date(y, m, 0).getDate()).padStart(2, "0")}`;
  const page = useAsync(
    () => api.queryTransactions({ categoryId, from, to, excludeTransfers: true, limit: LIMIT }),
    [categoryId, from, to],
  );
  const categories = useAsync(() => api.listCategories(), []);
  const [error, setError] = useState<string>();

  const move = async (id: number, newCategory: number | null) => {
    try {
      await api.setTransactionsCategory([id], newCategory);
      setError(undefined);
      page.reload();
      onChanged();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const p = page.data;
  const counted = p ? p.outflowCents - p.inflowCents : 0;

  return (
    <Modal title={`${categoryName} in ${monthLabel}`} onClose={onClose} wide>
      {page.error ? <p className="text-sm text-red-600">{page.error}</p> : null}
      {error ? <p className="mb-2 text-sm text-red-600">{error}</p> : null}
      {p && p.items.length === 0 ? (
        <p className="text-sm text-zinc-500">Nothing is counted in {categoryName} this month.</p>
      ) : null}
      {p && p.items.length > 0 ? (
        <div className="space-y-3">
          <div className="max-h-[60vh] overflow-y-auto rounded-lg ring-1 ring-zinc-200 dark:ring-zinc-800">
            <table className="w-full text-sm">
              <thead className="sticky top-0 bg-zinc-50 text-left text-xs text-zinc-500 dark:bg-zinc-800">
                <tr>
                  <th className="px-3 py-2 font-medium">Date</th>
                  <th className="px-3 py-2 font-medium">Payee</th>
                  <th className="px-3 py-2 font-medium">Category</th>
                  <th className="px-3 py-2 text-right font-medium">Amount</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
                {p.items.map((t) => (
                  <tr key={t.id}>
                    <td className="px-3 py-1.5 whitespace-nowrap text-zinc-500">{formatDate(t.postedOn)}</td>
                    <td className="px-3 py-1.5">
                      {t.payee}
                      <span className="block text-xs text-zinc-400">{t.accountName}</span>
                    </td>
                    <td className="px-3 py-1">
                      {categories.data ? (
                        <CategorySelect
                          categories={categories.data}
                          value={t.categoryId}
                          onChange={(cid) => move(t.id, cid)}
                          className="w-36 rounded-md border-0 bg-transparent px-1.5 py-1 text-sm hover:ring-1 hover:ring-zinc-300 focus:ring-2 focus:ring-brand-500"
                          aria-label={`Category for ${t.payee}`}
                        />
                      ) : null}
                    </td>
                    <td className="px-3 py-1.5 text-right whitespace-nowrap">
                      <Money cents={t.amountCents} colored />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="flex items-baseline justify-between text-sm">
            <span className="text-zinc-500">
              {p.total} transaction{p.total === 1 ? "" : "s"}
              {p.inflowCents > 0 ? (
                <>
                  {" "}
                  · refunds of <Money cents={p.inflowCents} /> taken off
                </>
              ) : null}
              {p.total > p.items.length ? ` · showing the latest ${p.items.length}` : ""}
            </span>
            <span className="font-medium">
              Counted: <Money cents={counted} />
            </span>
          </div>
          <p className="text-xs text-zinc-500">
            Change a category here and the budget updates. Transfers tagged to another account aren't counted, even
            when they carry this category.
          </p>
        </div>
      ) : null}
    </Modal>
  );
}
