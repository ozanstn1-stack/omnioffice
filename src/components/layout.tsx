import type { ReactNode } from "react";
import { useNarrowLayout } from "../lib/viewport";

/**
 * The shared screen shell.
 *
 * On a desktop window the header carries the screen's actions on the right. On
 * a phone-sized window (D8) the actions move out of the header and into a
 * bottom action bar: primary buttons stay inside the thumb's reach instead of
 * sitting at the top of a 360 dp screen, and the bar lifts itself above the
 * on-screen keyboard through `--keyboard-inset`. Screens that pass no actions
 * keep the plain scrolling page.
 */
export function Screen({
  title,
  subtitle,
  actions,
  children,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const narrow = useNarrowLayout();
  const content = (
    <div className="screen-content mx-auto px-7 py-6 flex flex-col gap-5" style={{ maxWidth: 1180 }}>
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <h1 className="text-[21px] font-bold tracking-tight leading-tight">{title}</h1>
          {subtitle ? <p className="muted text-[13.5px] mt-1">{subtitle}</p> : null}
        </div>
        {!narrow && actions ? <div className="flex items-center gap-2 shrink-0">{actions}</div> : null}
      </header>
      {children}
    </div>
  );
  if (!narrow) {
    return <div className="h-full overflow-y-auto overflow-x-hidden">{content}</div>;
  }
  return (
    <div className="screen-shell flex h-full flex-col">
      <div className="flex-1 overflow-y-auto overflow-x-hidden">{content}</div>
      {actions ? (
        <div className="screen-action-bar" role="toolbar">
          {actions}
        </div>
      ) : null}
    </div>
  );
}

export function TwoColumn({ main, side }: { main: ReactNode; side: ReactNode }) {
  return (
    <div className="two-col">
      <div className="col-main flex flex-col gap-4">{main}</div>
      <div className="col-side flex flex-col gap-4">{side}</div>
    </div>
  );
}

export function OptionCard({
  title,
  action,
  children,
  className = "",
}: {
  title?: ReactNode;
  /** Rendered next to the title, e.g. a "rescan" button. */
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`card p-4 flex flex-col gap-3.5 ${className}`}>
      {title || action ? (
        <div className="flex items-center justify-between gap-2">
          {title ? <h3 className="text-[13px] font-bold uppercase tracking-wider muted">{title}</h3> : null}
          {action}
        </div>
      ) : null}
      {children}
    </div>
  );
}
