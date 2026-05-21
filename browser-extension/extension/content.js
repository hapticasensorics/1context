chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || message.type !== "ONECONTEXT_COLLECT_PAGE_CONTEXT") {
    return false;
  }

  const selection = window.getSelection()?.toString() || "";
  const bodyText = document.body?.innerText || "";
  const domExcerpt = document.documentElement?.outerHTML || "";

  sendResponse({
    url: location.href,
    title: document.title,
    selectedText: selection.slice(0, 100000),
    visibleText: bodyText.slice(0, 200000),
    domExcerpt: domExcerpt.slice(0, 250000),
    viewport: {
      width: window.innerWidth,
      height: window.innerHeight,
      devicePixelRatio: window.devicePixelRatio,
      scrollX: window.scrollX,
      scrollY: window.scrollY
    },
    timestamp: new Date().toISOString()
  });

  return true;
});
