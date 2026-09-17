import { useCallback, useState, type ReactNode } from "react";
import { Modal } from "./Modal";
import { Button } from "./ui";

interface Pending {
  title: string;
  body: ReactNode;
  confirmLabel: string;
  resolve: (ok: boolean) => void;
}

/**
 * In-app confirmation dialog (the webview's native confirm() is not used).
 * Usage: const [confirm, dialog] = useConfirm(); ... if (await confirm({...})) ...; render {dialog}
 */
export function useConfirm() {
  const [pending, setPending] = useState<Pending>();

  const confirm = useCallback(
    (opts: { title: string; body: ReactNode; confirmLabel?: string }) =>
      new Promise<boolean>((resolve) =>
        setPending({ ...opts, confirmLabel: opts.confirmLabel ?? "Delete", resolve }),
      ),
    [],
  );

  const close = (ok: boolean) => {
    pending?.resolve(ok);
    setPending(undefined);
  };

  const dialog = pending ? (
    <Modal title={pending.title} onClose={() => close(false)}>
      <div className="space-y-5">
        <div className="text-sm text-zinc-600 dark:text-zinc-400">{pending.body}</div>
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={() => close(false)}>
            Cancel
          </Button>
          <Button variant="danger" onClick={() => close(true)}>
            {pending.confirmLabel}
          </Button>
        </div>
      </div>
    </Modal>
  ) : null;

  return [confirm, dialog] as const;
}
