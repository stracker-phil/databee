import { api, type DecodedValue } from "../../api";
import { useApiResult } from "../../lib/useApiResult";
import { fail } from "../../state/store";

/** PHP-serialized or JSON text decoded by Rust once `enabled`; null for plain text, while decoding and after a failure (toasted). */
export function useDecoded(text: string, enabled = true): DecodedValue | null {
  const { data } = useApiResult(enabled ? text : null, () => api.decodeValue(text), { onError: (e) => fail(e, "Could not decode the value") });
  return data;
}
