import { useSyncExternalStore } from "react";
import { errorDiagnostics, subscribeErrors } from "../i18n/errors";
import { useLocale } from "../i18n/LocaleContext";
import { sanitizeNoticeMessage } from "../NoticeCenter";
export default function OperationErrorDetails() {
  const entries = useSyncExternalStore(subscribeErrors, errorDiagnostics, errorDiagnostics);
  const { t } = useLocale();
  if (!entries.length) return null;
  return <section className="diagnostic-preview"><h3>{t("completion.operation_error_details")}</h3><p>{t("completion.original_diagnostic_details")}</p>{entries.slice().reverse().map(entry => <details key={entry.id}><summary>{entry.operation} · {entry.at}</summary><pre style={{whiteSpace:"pre-wrap", overflowWrap:"anywhere"}}>{sanitizeNoticeMessage(entry.detail)}</pre></details>)}</section>;
}
