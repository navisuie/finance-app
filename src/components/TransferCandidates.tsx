import { useAsync } from "../hooks/useAsync";
import { api, errorMessage } from "../lib/api";
import { formatDate } from "../lib/money";
import { Money } from "./privacy";
import { Button, Card } from "./ui";

/** Possible transfers between two tracked accounts, surfaced for review —
 * see PFA.md "Transfer auto-linking". Linking/dismissing never happens
 * silently. */
export function TransferCandidates({ onChanged }: { onChanged?: () => void }) {
  const candidates = useAsync(() => api.listTransferCandidates(), []);

  if (!candidates.data || candidates.data.length === 0) return null;

  async function act(fn: () => Promise<void>) {
    try {
      await fn();
      await candidates.reload();
      onChanged?.();
    } catch (e) {
      window.alert(errorMessage(e));
    }
  }

  return (
    <Card title="Possible transfers">
      <ul className="space-y-2 text-sm">
        {candidates.data.map(({ a, b }) => (
          <li key={`${a.id}-${b.id}`} className="flex items-center justify-between gap-3">
            <span>
              <Money cents={Math.abs(a.amountCents)} /> — {a.accountName} ({formatDate(a.postedOn)}) &rarr;{" "}
              {b.accountName} ({formatDate(b.postedOn)})
            </span>
            <span className="flex shrink-0 gap-2">
              <Button variant="secondary" className="py-1" onClick={() => act(() => api.confirmTransferLink(a.id, b.id))}>
                Link
              </Button>
              <Button variant="secondary" className="py-1" onClick={() => act(() => api.dismissTransferCandidate(a.id, b.id))}>
                Not a transfer
              </Button>
            </span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
