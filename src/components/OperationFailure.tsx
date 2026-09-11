import { useLocale } from "../i18n/LocaleContext";
import { errorMessage } from "../i18n/errors";
import { sanitizeNoticeMessage } from "../NoticeCenter";
export default function OperationFailure({ reason }: { reason: string }) {
  const { t, locale } = useLocale();
  return <div className="operation-failure"><p>{errorMessage(reason, locale)}</p><details><summary>{t("completion.operation_error_details")}</summary><pre style={{whiteSpace:"pre-wrap",overflowWrap:"anywhere"}}>{sanitizeNoticeMessage(reason)}</pre></details></div>;
}
