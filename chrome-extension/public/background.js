// Service worker: opens the toolkit in a tab. The extension has no host
// permissions: a right-clicked page is read only through activeTab, which
// Chrome grants for that one click on that one tab.
const APP_URL = "app.html";

// Mirrors MAX_SELECTION_CHARS in src/lib/selection.ts (keeps the URL small).
const MAX_SELECTION_CHARS = 150000;

const PDF_URLS = ["*://*/*.pdf", "*://*/*.PDF", "*://*/*.pdf?*", "*://*/*.PDF?*"];

const message = (key, fallback) => {
  try {
    return chrome.i18n.getMessage(key) || fallback;
  } catch {
    return fallback;
  }
};

const isPdfUrl = (url) => {
  try {
    return new URL(url).pathname.toLowerCase().endsWith(".pdf");
  } catch {
    return false;
  }
};

async function focusOrOpen(fragment) {
  const url = chrome.runtime.getURL(APP_URL);
  // A nonce makes every hand-off a new hash, so a reused tab notices it even
  // when the same link or text is opened twice in a row.
  const target = fragment ? `${url}#${fragment}&n=${Date.now()}` : url;
  // tabs.query cannot match the extension's own pages; getContexts lists them.
  const contexts = (await chrome.runtime.getContexts?.({ contextTypes: ["TAB"] }).catch(() => [])) ?? [];
  const existing = contexts.find((context) => context.tabId >= 0 && context.documentUrl?.startsWith(url));
  if (existing) {
    try {
      await chrome.tabs.update(existing.tabId, fragment ? { url: target, active: true } : { active: true });
      if (existing.windowId >= 0) await chrome.windows.update(existing.windowId, { focused: true });
      return;
    } catch {
      // The tab closed in the meantime: open a new one below.
    }
  }
  await chrome.tabs.create({ url: target });
}

/**
 * Reads the selection (with its line breaks) or, when nothing is selected, the
 * readable text of the page. Runs in the clicked tab through activeTab.
 */
function readPageText(wantSelection) {
  const selection = window.getSelection()?.toString() ?? "";
  if (wantSelection && selection.trim()) return { title: document.title, text: selection };
  const root = document.querySelector("article") ?? document.querySelector("main") ?? document.body;
  return { title: document.title, text: root?.innerText ?? "" };
}

async function pageText(tab, wantSelection) {
  if (tab?.id === undefined) return null;
  try {
    const [result] = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: readPageText,
      args: [wantSelection],
    });
    return result?.result ?? null;
  } catch {
    // Pages Chrome does not let extensions read (chrome://, the Web Store,
    // the built-in PDF viewer) end up here.
    return null;
  }
}

function textFragment(text, title, pageUrl) {
  const params = new URLSearchParams({ text: text.slice(0, MAX_SELECTION_CHARS) });
  if (title) params.set("title", title.slice(0, 200));
  if (pageUrl) params.set("src", pageUrl);
  return params.toString();
}

chrome.action.onClicked.addListener(() => focusOrOpen(""));

chrome.runtime.onInstalled.addListener(() => {
  // Menus persist across updates: drop the old entries first.
  chrome.contextMenus.removeAll(() => {
    // Selected text, or the whole page when nothing is selected (Chrome shows
    // page entries only when no selection, link or image was clicked).
    chrome.contextMenus.create({
      id: "omnioffice-open-text",
      title: message("menuOpenAsPdf", "Open as PDF with OmniOffice"),
      contexts: ["selection", "page"],
    });
    chrome.contextMenus.create({
      id: "omnioffice-open-link",
      title: message("menuOpenPdf", "Open PDF with OmniOffice"),
      contexts: ["link"],
      targetUrlPatterns: PDF_URLS,
    });
    chrome.contextMenus.create({
      id: "omnioffice-open-any",
      title: message("menuOpenApp", "Open OmniOffice"),
      contexts: ["action"],
    });
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId === "omnioffice-open-link" && info.linkUrl) {
    return focusOrOpen(new URLSearchParams({ url: info.linkUrl }).toString());
  }
  if (info.menuItemId !== "omnioffice-open-text") return focusOrOpen("");
  const selected = Boolean(info.selectionText);
  // A PDF page itself is downloaded and opened as it is, not re-typeset.
  if (!selected && info.pageUrl && isPdfUrl(info.pageUrl)) {
    return focusOrOpen(new URLSearchParams({ url: info.pageUrl }).toString());
  }
  const read = await pageText(tab, selected);
  const text = read?.text?.trim() ? read.text : (info.selectionText ?? "");
  if (!text.trim()) {
    return focusOrOpen(new URLSearchParams({ textError: "unreadable" }).toString());
  }
  return focusOrOpen(textFragment(text, read?.title ?? tab?.title ?? "", info.pageUrl));
});
