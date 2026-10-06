import { BookOpen, Bot, LayoutGrid, Sparkles, X } from "lucide-react";
import { Button, Card, IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { useSettings } from "../lib/store";
import type { Navigate, ScreenId } from "../lib/nav";

const STEPS: { key: "office" | "pdf" | "ai"; screen: ScreenId; icon: React.ReactNode }[] = [
  { key: "office", screen: "office", icon: <LayoutGrid size={18} /> },
  { key: "pdf", screen: "reader", icon: <BookOpen size={18} /> },
  { key: "ai", screen: "ai", icon: <Bot size={18} /> },
];

/**
 * First-run guide on Home: the office suite, the PDF tools and the optional
 * AI setup, each with a button that goes there. It stays until the user
 * closes it; Settings can bring it back.
 */
export function WelcomeCard({ onNavigate }: { onNavigate: Navigate }) {
  const t = useT();
  const loaded = useSettings((state) => state.loaded);
  const done = useSettings((state) => state.settings.onboardingDone);
  const update = useSettings((state) => state.update);

  if (!loaded || done) return null;

  const finish = () => void update({ onboardingDone: true });

  return (
    <section aria-labelledby="welcome-title">
      <Card className="p-4 fade-in">
        <div className="flex items-start gap-3 mb-3">
          <Sparkles size={18} style={{ color: "var(--accent)", marginTop: 2 }} aria-hidden />
          <div className="flex-1 min-w-0">
            <h2 id="welcome-title" className="font-semibold text-[15px]">
              {t("welcome.title")}
            </h2>
            <p className="text-xs muted mt-1">{t("welcome.intro")}</p>
          </div>
          <IconButton label={t("common.close")} onClick={finish}>
            <X size={16} />
          </IconButton>
        </div>
        <ol className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
          {STEPS.map((step, index) => (
            <li
              key={step.key}
              className="flex flex-col gap-2 rounded-lg p-3"
              style={{ background: "var(--surface-2)" }}
            >
              <span className="flex items-center gap-2 font-semibold text-[13.5px]">
                <span style={{ color: "var(--accent)" }} aria-hidden>
                  {step.icon}
                </span>
                {index + 1}. {t(`welcome.${step.key}Title`)}
              </span>
              <span className="text-xs muted leading-relaxed flex-1">{t(`welcome.${step.key}Body`)}</span>
              <Button size="sm" onClick={() => onNavigate(step.screen)}>
                {t(`welcome.${step.key}Action`)}
              </Button>
            </li>
          ))}
        </ol>
        <div className="flex justify-end mt-3">
          <Button size="sm" variant="primary" onClick={finish}>
            {t("welcome.done")}
          </Button>
        </div>
      </Card>
    </section>
  );
}
