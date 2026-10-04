// Service worker: opens the toolkit in a tab. There is no network usage and
// no host permissions - the extension is fully offline.
const APP_URL = "app.html";

chrome.action.onClicked.addListener(async () => {
  const url = chrome.runtime.getURL(APP_URL);
  const existing = await chrome.tabs.query({ url });
  if (existing.length && existing[0].id !== undefined) {
    await chrome.tabs.update(existing[0].id, { active: true });
    if (existing[0].windowId !== undefined) {
      await chrome.windows.update(existing[0].windowId, { focused: true });
    }
    return;
  }
  await chrome.tabs.create({ url });
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "pdfsak-open",
    title: "Open with OmniOffice",
    contexts: ["link", "page"],
    targetUrlPatterns: ["*://*/*.pdf", "*://*/*.PDF"],
  });
  chrome.contextMenus.create({
    id: "pdfsak-open-any",
    title: "Open OmniOffice",
    contexts: ["action"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  const url = chrome.runtime.getURL(APP_URL);
  const suffix = info.linkUrl || info.pageUrl ? `#url=${encodeURIComponent(info.linkUrl || info.pageUrl || "")}` : "";
  const existing = await chrome.tabs.query({ url });
  if (existing.length && existing[0].id !== undefined) {
    await chrome.tabs.update(existing[0].id, { url: `${url}${suffix}`, active: true });
    return;
  }
  await chrome.tabs.create({ url: `${url}${suffix}` });
});
