import { useId, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";

export function Button({
  variant = "primary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" | "ghost" | "danger" }) {
  const styles = {
    primary: "bg-brand-600 text-white hover:bg-brand-700 disabled:bg-brand-600/50",
    secondary:
      "bg-white text-zinc-900 ring-1 ring-zinc-300 hover:bg-zinc-50 dark:bg-zinc-900 dark:text-zinc-100 dark:ring-zinc-700 dark:hover:bg-zinc-800",
    ghost: "text-zinc-600 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800",
    danger: "bg-red-600 text-white hover:bg-red-700",
  }[variant];
  return (
    <button
      className={`inline-flex items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition disabled:cursor-not-allowed ${styles} ${className}`}
      {...props}
    />
  );
}

export function Field({
  label,
  hint,
  error,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode; error?: string }) {
  const id = useId();
  return (
    <div className="space-y-1.5">
      <label htmlFor={id} className="block text-sm font-medium text-zinc-700 dark:text-zinc-300">
        {label}
      </label>
      <input
        id={id}
        className="block w-full rounded-lg border-0 bg-white px-3 py-2 text-sm ring-1 ring-zinc-300 outline-none focus:ring-2 focus:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-700"
        aria-invalid={error ? true : undefined}
        {...props}
      />
      {error ? <p className="text-sm text-red-600 dark:text-red-400">{error}</p> : null}
      {hint && !error ? <div className="text-xs text-zinc-500">{hint}</div> : null}
    </div>
  );
}

export function PasswordField(props: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode; error?: string }) {
  const [show, setShow] = useState(false);
  return (
    <div className="relative">
      <Field
        {...props}
        type={show ? "text" : "password"}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
      />
      <button
        type="button"
        onClick={() => setShow((s) => !s)}
        className="absolute top-0 right-0 text-xs font-medium text-brand-700 hover:underline dark:text-brand-500"
      >
        {show ? "Hide" : "Show"}
      </button>
    </div>
  );
}

export function Card({ title, children, className = "" }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <section
      className={`rounded-xl bg-white p-5 shadow-sm ring-1 ring-zinc-200 dark:bg-zinc-900 dark:ring-zinc-800 ${className}`}
    >
      {title ? <h2 className="mb-3 text-sm font-semibold text-zinc-500 dark:text-zinc-400">{title}</h2> : null}
      {children}
    </section>
  );
}

export function CenteredPanel({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-full items-center justify-center p-6">
      <div className="w-full max-w-md rounded-2xl bg-white p-8 shadow-lg ring-1 ring-zinc-200 dark:bg-zinc-900 dark:ring-zinc-800">
        {children}
      </div>
    </div>
  );
}

export function Logo({ size = 40 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 40 40" aria-hidden="true">
      <rect width="40" height="40" rx="9" className="fill-brand-900" />
      <rect x="9" y="21" width="4" height="10" rx="1" className="fill-brand-500" />
      <rect x="15" y="17" width="4" height="14" rx="1" className="fill-brand-500" />
      <rect x="21" y="13" width="4" height="18" rx="1" className="fill-brand-500" />
      <rect x="27" y="9" width="4" height="22" rx="1" className="fill-brand-500" />
    </svg>
  );
}
