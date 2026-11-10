import React from "react";

// Route-level code splitting: every screen is loaded when it is first opened,
// so the startup bundle only carries the shell, the navigation and the shared
// UI primitives. The lazy import maps the named export to the default export
// `React.lazy` expects.
export const HomeScreen = React.lazy(() => import("./Home").then((module) => ({ default: module.Home })));
export const Reader = React.lazy(() => import("./Reader").then((module) => ({ default: module.Reader })));
export const Ai = React.lazy(() => import("./Ai").then((module) => ({ default: module.Ai })));
export const AiPacks = React.lazy(() => import("./AiPacks").then((module) => ({ default: module.AiPacks })));
export const AiLibrary = React.lazy(() => import("./AiLibrary").then((module) => ({ default: module.AiLibrary })));
export const Merge = React.lazy(() => import("./Merge").then((module) => ({ default: module.Merge })));
export const Organize = React.lazy(() => import("./Organize").then((module) => ({ default: module.Organize })));
export const Split = React.lazy(() => import("./Split").then((module) => ({ default: module.Split })));
// Phones only; until it loads an empty bar of the same height holds its place.
// The sidebar and the drawer are their own chunk (7.31) so the entry stays in budget.
export const DesktopSidebar = React.lazy(() =>
  import("../components/sidebar").then((module) => ({ default: module.DesktopSidebar })),
);
export const DrawerNav = React.lazy(() =>
  import("../components/sidebar").then((module) => ({ default: module.DrawerNav })),
);
export const OfficeLauncher = React.lazy(() =>
  import("./OfficeLauncher").then((module) => ({ default: module.OfficeLauncher })),
);
export const BottomNav = React.lazy(() =>
  import("../components/bottom-nav").then((module) => ({ default: module.BottomNav })),
);
export const PresendDialog = React.lazy(() =>
  import("../components/presend-dialog").then((module) => ({ default: module.PresendDialog })),
);
export const Compress = React.lazy(() => import("./Compress").then((module) => ({ default: module.Compress })));
export const Ocr = React.lazy(() => import("./Ocr").then((module) => ({ default: module.Ocr })));
export const Convert = React.lazy(() => import("./Convert").then((module) => ({ default: module.Convert })));
export const Security = React.lazy(() => import("./Security").then((module) => ({ default: module.Security })));
export const Watermark = React.lazy(() => import("./Watermark").then((module) => ({ default: module.Watermark })));
export const PageTools = React.lazy(() => import("./PageTools").then((module) => ({ default: module.PageTools })));
export const Annotate = React.lazy(() => import("./Annotate").then((module) => ({ default: module.Annotate })));
export const Redact = React.lazy(() => import("./Redact").then((module) => ({ default: module.Redact })));
export const Compare = React.lazy(() => import("./Compare").then((module) => ({ default: module.Compare })));
export const Inspect = React.lazy(() => import("./Inspect").then((module) => ({ default: module.Inspect })));
export const Metadata = React.lazy(() => import("./Metadata").then((module) => ({ default: module.Metadata })));
export const Batch = React.lazy(() => import("./Batch").then((module) => ({ default: module.Batch })));
export const History = React.lazy(() => import("./History").then((module) => ({ default: module.History })));
export const Settings = React.lazy(() => import("./Settings").then((module) => ({ default: module.Settings })));
export const InfoScreen = React.lazy(() => import("./Info").then((module) => ({ default: module.InfoScreen })));
export const Vault = React.lazy(() => import("./Vault").then((module) => ({ default: module.Vault })));
export const PdfStudio = React.lazy(() => import("./PdfStudio").then((module) => ({ default: module.PdfStudio })));
export const CompatibilityScreen = React.lazy(() =>
  import("./Compatibility").then((module) => ({ default: module.CompatibilityScreen })),
);
export const JobsScreen = React.lazy(() => import("./Jobs").then((module) => ({ default: module.JobsScreen })));
export const Sync = React.lazy(() => import("./Sync").then((module) => ({ default: module.Sync })));
export const Plugins = React.lazy(() => import("./Plugins").then((module) => ({ default: module.Plugins })));
export const OfficeWorkspace = React.lazy(() =>
  import("../office/OfficeWorkspace").then((module) => ({ default: module.OfficeWorkspace })),
);
export const CleanerScreen = React.lazy(() =>
  import("../office/tools/cleaner").then((module) => ({ default: module.CleanerScreen })),
);
export const ConverterScreen = React.lazy(() =>
  import("../office/tools/converter").then((module) => ({ default: module.ConverterScreen })),
);
export const InvoicesScreen = React.lazy(() => import("./Invoices").then((module) => ({ default: module.Invoices })));
export const EmlScreen = React.lazy(() => import("./Eml").then((module) => ({ default: module.Eml })));
export const ScannerScreen = React.lazy(() => import("./Scanner").then((module) => ({ default: module.Scanner })));
export const RecipesScreen = React.lazy(() => import("./Recipes").then((module) => ({ default: module.Recipes })));
export const MailMergeScreen = React.lazy(() =>
  import("./MailMerge").then((module) => ({ default: module.MailMerge })),
);
export const DataScreen = React.lazy(() =>
  import("../office/tools/data").then((module) => ({ default: module.DataScreen })),
);
export const DrawScreen = React.lazy(() =>
  import("../office/tools/draw").then((module) => ({ default: module.DrawScreen })),
);
export const NotesScreen = React.lazy(() =>
  import("../office/tools/notes").then((module) => ({ default: module.NotesScreen })),
);
export const PdfFormsScreen = React.lazy(() =>
  import("../office/tools/pdf-forms").then((module) => ({ default: module.PdfFormsScreen })),
);
export const PlannerScreen = React.lazy(() =>
  import("../office/tools/planner").then((module) => ({ default: module.PlannerScreen })),
);
export const TemplatesScreen = React.lazy(() =>
  import("../office/tools/templates").then((module) => ({ default: module.TemplatesScreen })),
);
export const DeadlineCalculatorScreen = React.lazy(() =>
  import("./DeadlineCalculator").then((module) => ({ default: module.DeadlineCalculatorScreen })),
);
export const OfficialLetterScreen = React.lazy(() =>
  import("./OfficialLetterWizard").then((module) => ({ default: module.OfficialLetterWizard })),
);
export const EvidenceBundleScreen = React.lazy(() =>
  import("./EvidenceBundle").then((module) => ({ default: module.EvidenceBundleScreen })),
);
export const PetitionWizardScreen = React.lazy(() =>
  import("./PetitionWizard").then((module) => ({ default: module.PetitionWizard })),
);
export const KvkkWizardScreen = React.lazy(() =>
  import("./KvkkWizard").then((module) => ({ default: module.KvkkWizard })),
);
export const DocumentHealthScreen = React.lazy(() =>
  import("./DocumentHealthScreen").then((module) => ({ default: module.DocumentHealthScreen })),
);
export const ToolsScreen = React.lazy(() => import("./Tools").then((module) => ({ default: module.ToolsScreen })));
export const VerifyScreen = React.lazy(() => import("./Verify").then((module) => ({ default: module.VerifyScreen })));
