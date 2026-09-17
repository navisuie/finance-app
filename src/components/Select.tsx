import { useId, type SelectHTMLAttributes } from "react";

/** Input/select styling without width, for inline controls. */
export const controlClass =
  "rounded-lg border-0 bg-white px-2.5 py-2 text-sm ring-1 ring-zinc-300 outline-none focus:ring-2 focus:ring-brand-500 dark:bg-zinc-900 dark:ring-zinc-700";

/** Full-width form control. */
export const selectClass = `block w-full ${controlClass}`;

export function Select({
  label,
  options,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement> & {
  label?: string;
  options: { value: string | number; label: string }[];
}) {
  const id = useId();
  return (
    <div className="space-y-1.5">
      {label ? (
        <label htmlFor={id} className="block text-sm font-medium text-zinc-700 dark:text-zinc-300">
          {label}
        </label>
      ) : null}
      <select id={id} className={selectClass} {...props}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </div>
  );
}
