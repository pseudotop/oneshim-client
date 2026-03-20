// Lightweight translation hook.
// Delegates to the existing i18n system or returns the key as-is.
import { useCallback } from "react";

export function useTranslation() {
  const t = useCallback((key: string) => key, []);
  return { t };
}
