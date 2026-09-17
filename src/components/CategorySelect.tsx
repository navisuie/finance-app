import type { Category } from "../lib/types";
import { selectClass } from "./Select";

const KIND_TITLES = { income: "Income", expense: "Spending", transfer: "Transfers" } as const;

export function CategorySelect({
  categories,
  value,
  onChange,
  allowNone = true,
  noneLabel = "Uncategorized",
  className = selectClass,
  ...rest
}: {
  categories: Category[];
  value: number | null;
  onChange: (id: number | null) => void;
  allowNone?: boolean;
  noneLabel?: string;
  className?: string;
  "aria-label"?: string;
  id?: string;
}) {
  return (
    <select
      className={className}
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
      {...rest}
    >
      {allowNone ? <option value="">{noneLabel}</option> : null}
      {(["income", "expense", "transfer"] as const).map((kind) => (
        <optgroup key={kind} label={KIND_TITLES[kind]}>
          {categories
            .filter((c) => c.kind === kind)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </optgroup>
      ))}
    </select>
  );
}
