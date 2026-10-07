import { Component, Fragment, useState, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { logFrontend } from "../lib/api";
import { useT } from "../lib/i18n";
import { Button, EmptyState } from "./ui";

/** Keeps a runaway stack from flooding frontend.log or the clipboard. */
const MAX_DETAIL_CHARS = 4000;

function toError(value: unknown): Error {
  return value instanceof Error ? value : new Error(String(value));
}

/**
 * Plain-text crash report used for the log line and "Copy details": the scope,
 * the error with its stack and the React component stack. It never contains
 * document content - only what the runtime itself reports about the failure.
 */
export function describeCrash(scope: string, error: unknown, componentStack?: string | null): string {
  const failure = toError(error);
  const headline = `${failure.name}: ${failure.message}`;
  // V8 stacks already start with the headline; other engines' do not.
  const stack = failure.stack
    ? failure.stack.includes(failure.message)
      ? failure.stack
      : `${headline}\n${failure.stack}`
    : headline;
  const parts = [`scope: ${scope}`, stack];
  if (componentStack?.trim()) parts.push(`component stack:${componentStack}`);
  return parts.join("\n").slice(0, MAX_DETAIL_CHARS);
}

interface ErrorBoundaryProps {
  /** Where the boundary sits, e.g. `screen:ocr` or `office:writer`; goes into the log. */
  scope: string;
  children: ReactNode;
  /** Extra sentence under the message (for example what happened to unsaved work). */
  note?: string;
  /** Runs once per caught error; a throwing handler is ignored. */
  onError?: (error: Error) => void;
}

interface ErrorBoundaryState {
  error: Error | null;
  componentStack: string;
  /** Bumped by "Try again" so the children remount from scratch. */
  attempt: number;
}

/**
 * Keeps a render crash inside the frame it happened in: the shell, the
 * navigation and the other Office tabs stay usable, the failure is written to
 * frontend.log (which the diagnostics export includes) and the user can retry
 * or copy the details for a bug report.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null, componentStack: "", attempt: 0 };

  static getDerivedStateFromError(error: unknown): Partial<ErrorBoundaryState> {
    return { error: toError(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    const failure = toError(error);
    const componentStack = info.componentStack ?? "";
    this.setState({ componentStack });
    void logFrontend("error", `render crash ${describeCrash(this.props.scope, failure, componentStack)}`);
    try {
      this.props.onError?.(failure);
    } catch {
      // Recovery is best effort: a failing handler must not hide the fallback.
    }
  }

  private retry = () => {
    this.setState((state) => ({ error: null, componentStack: "", attempt: state.attempt + 1 }));
  };

  render() {
    const { error, componentStack, attempt } = this.state;
    if (error) {
      return (
        <CrashFallback
          details={describeCrash(this.props.scope, error, componentStack)}
          message={error.message}
          note={this.props.note}
          onRetry={this.retry}
        />
      );
    }
    return <Fragment key={attempt}>{this.props.children}</Fragment>;
  }
}

function CrashFallback({
  details,
  message,
  note,
  onRetry,
}: {
  details: string;
  message: string;
  note?: string;
  onRetry: () => void;
}) {
  const t = useT();
  const [copied, setCopied] = useState(false);

  const copy = () => {
    // The clipboard can be unavailable (permissions, insecure context); the
    // button then simply stays as it was.
    void Promise.resolve()
      .then(() => navigator.clipboard.writeText(details))
      .then(() => setCopied(true))
      .catch(() => undefined);
  };

  return (
    <div role="alert" className="h-full overflow-auto">
      <EmptyState
        icon={<AlertTriangle size={24} />}
        title={t("errors.title")}
        hint={[t("errors.boundaryBody"), note].filter(Boolean).join(" ")}
        action={
          <>
            <code className="block max-w-md text-xs muted break-words">{message}</code>
            <div className="flex gap-2">
              <Button variant="primary" onClick={onRetry}>
                {t("errors.tryAgain")}
              </Button>
              <Button onClick={copy}>{copied ? t("errors.copied") : t("errors.copyDetails")}</Button>
            </div>
          </>
        }
      />
    </div>
  );
}
