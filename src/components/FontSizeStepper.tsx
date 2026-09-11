import { useLocale } from "../i18n/LocaleContext";

import {
  stepDiscussionFontSize,
  type DiscussionFontSize,
} from "../discussionFont";

export default function FontSizeStepper({
  value,
  onChange,
}: {
  value: DiscussionFontSize;
  onChange: (value: DiscussionFontSize) => void;
}) {
  const { t } = useLocale();
  return (
    <div className="font-size-stepper" role="group" aria-label={t("reader.font.group")}>
      <button
        type="button"
        className="icon-button"
        aria-label={t("reader.font.decrease")}
        disabled={value <= 14}
        onClick={() => onChange(stepDiscussionFontSize(value, -1))}
      >
        A−
      </button>
      <span>{value}</span>
      <button
        type="button"
        className="icon-button"
        aria-label={t("reader.font.increase")}
        disabled={value >= 17}
        onClick={() => onChange(stepDiscussionFontSize(value, 1))}
      >
        A+
      </button>
    </div>
  );
}
