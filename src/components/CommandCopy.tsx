import { forwardRef, useState } from "react";
import { Copy } from "lucide-react";
import { useI18n } from "../i18n";
import { connectionFixCopy } from "../connectionFixCopy";

/** A terminal command the user runs themselves, with a copy button. */
export const CommandCopy = forwardRef<HTMLButtonElement, { command: string }>(
  function CommandCopy({ command }, ref) {
    const { locale } = useI18n();
    const [status, setStatus] = useState<"copied" | "copyFailed" | null>(null);
    const copyCommand = async () => {
      try {
        await navigator.clipboard.writeText(command);
        setStatus("copied");
      } catch {
        setStatus("copyFailed");
      }
    };
    return (
      <div className="command-copy">
        <code>{command}</code>
        <button
          ref={ref}
          type="button"
          className="button"
          onClick={() => void copyCommand()}
        >
          <Copy size={14} aria-hidden="true" />
          {connectionFixCopy(locale, "copy")}
        </button>
        {status && (
          <span role="status" className="field-help">
            {connectionFixCopy(locale, status)}
          </span>
        )}
      </div>
    );
  },
);
