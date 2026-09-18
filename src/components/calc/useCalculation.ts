import { useEffect, useState } from "react";
import { errorMessage } from "../../lib/api";

/** Recomputes a calculator result shortly after `input` stops changing —
 * "live" without hammering the backend on every keystroke. */
export function useCalculation<I, O>(calc: (input: I) => Promise<O>, input: I): { result?: O; error?: string } {
  const [result, setResult] = useState<O>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    const t = setTimeout(() => {
      calc(input)
        .then((r) => {
          if (cancelled) return;
          setResult(r);
          setError(undefined);
        })
        .catch((e) => !cancelled && setError(errorMessage(e)));
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(input)]);

  return { result, error };
}
