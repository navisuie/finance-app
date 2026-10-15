import { useEffect, useState, type ReactNode } from "react";
import { centsToInput, parseMoney } from "../../lib/money";
import { Field } from "../ui";

/**
 * Keeps whatever you typed for as long as the field has focus.
 *
 * Without this, a field fights you: "500" parses to 50000 cents, the parent
 * re-renders, and the formatted value "500.00" replaces what you were
 * halfway through typing — so deleting a digit silently re-adds the decimal
 * and the zeros, and the caret jumps to the end. The tidy form belongs at
 * the end of an edit, not during one, so the value from outside is only
 * applied while the field is idle (which is also what makes loading a saved
 * scenario work).
 */
function useEditableText(formatted: string) {
  const [text, setText] = useState(formatted);
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setText(formatted);
  }, [formatted, editing]);
  return { text, setText, setEditing };
}

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
  const { text, setText, setEditing } = useEditableText(centsToInput(cents));
  return (
    <Field
      label={label}
      value={text}
      inputMode="decimal"
      hint={hint}
      onFocus={() => setEditing(true)}
      // Leaving the field tidies it up, and anything unparseable (an empty
      // box, a stray letter) falls back to the last good value.
      onBlur={() => setEditing(false)}
      onChange={(e) => {
        setText(e.target.value);
        const c = parseMoney(e.target.value);
        if (c !== null) onChange(c);
      }}
    />
  );
}

/** A plain number field (age, years, a percent) — no cents conversion.
 *
 * Deliberately a text input with a decimal keypad rather than
 * `type="number"`: a controlled number input reports an empty value for a
 * half-typed "6.", so typing a decimal into a percentage used to reset the
 * field. Range is enforced here on blur instead of by the browser. */
export function NumberField({
  label,
  value,
  onChange,
  min,
  max,
  hint,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  hint?: ReactNode;
}) {
  const { text, setText, setEditing } = useEditableText(Number.isFinite(value) ? String(value) : "");
  const clamp = (n: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, n));
  return (
    <Field
      label={label}
      value={text}
      inputMode="decimal"
      hint={hint}
      onFocus={() => setEditing(true)}
      onBlur={() => {
        const n = Number(text);
        if (text.trim() !== "" && Number.isFinite(n) && clamp(n) !== n) onChange(clamp(n));
        setEditing(false);
      }}
      onChange={(e) => {
        setText(e.target.value);
        // A lone "-", "." or "" is a half-typed number, not a change yet.
        const n = Number(e.target.value);
        if (e.target.value.trim() !== "" && Number.isFinite(n)) onChange(n);
      }}
    />
  );
}
