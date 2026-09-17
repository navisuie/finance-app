import { useEffect, useRef, type ReactNode } from "react";

export function Modal({
  title,
  onClose,
  children,
  wide,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Kept in a ref so a caller passing an inline onClose doesn't re-run the
  // effect below on every render, which would steal focus while typing.
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeRef.current();
    };
    window.addEventListener("keydown", onKey);
    // Focus the first field once, when the dialog opens. Fields come before
    // buttons so typing starts where you'd expect.
    const box = ref.current;
    const field = box?.querySelector<HTMLElement>(
      "input:not([type='hidden']):not([type='checkbox']), select, textarea",
    );
    (field ?? box?.querySelector<HTMLElement>("button"))?.focus();
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/40 p-6" onMouseDown={onClose}>
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        className={`mt-10 w-full ${wide ? "max-w-5xl" : "max-w-lg"} rounded-2xl bg-white p-6 shadow-xl ring-1 ring-zinc-200 dark:bg-zinc-900 dark:ring-zinc-800`}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold">{title}</h2>
          <button onClick={onClose} className="rounded p-1 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" aria-label="Close">
            ✕
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
