import { useEffect, useState, type ReactNode } from "react";
import { centsToInput, parseMoney } from "../../lib/money";
import { Field } from "../ui";

/** A dollar-amount field backed by an integer-cents value, matching the
 * money.ts conventions used elsewhere (TransactionForm, AccountForm). */
export function MoneyField({
  label,
  cents,
  onChange,
  hint,
}: {
  label: string;
  cents: number;
  onChange: (cents: number) => void;
  hint?: ReactNode;
}) {
  const [text, setText] = useState(centsToInput(cents));
  useEffect(() => setText(centsToInput(cents)), [cents]);
  return (
    <Field
      label={label}
      value={text}
      inputMode="decimal"
      hint={hint}
      onChange={(e) => {
        setText(e.target.value);
        const c = parseMoney(e.target.value);
        if (c !== null) onChange(c);
      }}
    />
  );
}

/** A plain number field (age, years, a percent) — no cents conversion. */
export function NumberField({
  label,
  value,
  onChange,
  step = 1,
  min,
  max,
  hint,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  step?: number;
  min?: number;
  max?: number;
  hint?: ReactNode;
}) {
  return (
    <Field
      label={label}
      type="number"
      value={Number.isFinite(value) ? value : ""}
      step={step}
      min={min}
      max={max}
      hint={hint}
      onChange={(e) => {
        const v = e.target.valueAsNumber;
        onChange(Number.isFinite(v) ? v : 0);
      }}
    />
  );
}
