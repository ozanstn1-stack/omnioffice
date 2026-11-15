// Service worker: opens the toolkit in a tab. There is no network usage and
// no host permissions - the extension is fully offline.
const APP_URL = "app.html";

// Mirrors MAX_SELECTION_CHARS in src/lib/selection.ts (keeps the URL small).
const MAX_SELECTION_CHARS = 50000;

const PDF_URLS = ["*://*/*.pdf", "*://*/*.PDF", "*://*/*.pdf?*", "*://*/*.PDF?*"];

async function focusOrOpen(fragment) {
  const url = chrome.runtime.getURL(APP_URL);
  // A nonce makes every hand-off a new hash, so a reused tab notices it even
  // when the same link or text is opened twice in a row.
  const target = fragment ? `${url}#${fragment}&n=${Date.now()}` : url;
  const existing = await chrome.tabs.query({ url });
  if (existing.length && existing[0].id !== undefined) {
    await chrome.tabs.update(existing[0].id, fragment ? { url: target, active: true } : { active: true });
    if (existing[0].windowId !== undefined) {
      await chrome.windows.update(existing[0].windowId, { focused: true });
    }
    return;
  }
  await chrome.tabs.create({ url: target });
}

chrome.action.onClicked.addListener(() => focusOrOpen(""));

chrome.runtime.onInstalled.addListener(() => {
  // Menus persist across updates: drop the old entries (1.0.3 showed "Open
  // with OmniOffice" on every page, which handed HTML pages to the reader).
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: "omnioffice-open-link",
      title: "Open PDF with OmniOffice",
      contexts: ["link"],
      targetUrlPatterns: PDF_URLS,
    });
    chrome.contextMenus.create({
      id: "omnioffice-open-page",
      title: "Open PDF with OmniOffice",
      contexts: ["page"],
      documentUrlPatterns: PDF_URLS,
    });
    chrome.contextMenus.create({
      id: "omnioffice-open-selection",
      title: "Open “%s” in OmniOffice",
      contexts: ["selection"],
    });
    chrome.contextMenus.create({
      id: "omnioffice-open-any",
      title: "Open OmniOffice",
      contexts: ["action"],
    });
  });
});

chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId === "omnioffice-open-selection" && info.selectionText) {
    const text = info.selectionText.slice(0, MAX_SELECTION_CHARS);
    const params = new URLSearchParams({ text });
    if (info.pageUrl) params.set("src", info.pageUrl);
    return focusOrOpen(params.toString());
  }
  const link = info.menuItemId === "omnioffice-open-link" ? info.linkUrl : info.pageUrl;
  if (info.menuItemId !== "omnioffice-open-any" && link) {
    return focusOrOpen(new URLSearchParams({ url: link }).toString());
  }
  return focusOrOpen("");
});
