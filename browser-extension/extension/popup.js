const HOST_NAME = "com.haptica.onecontext_browser_bridge";

function setStatus(value) {
  document.getElementById("status").textContent =
    typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

function manifestPermissionSet(manifest) {
  return Array.from(new Set([...(manifest.permissions || [])])).sort();
}

function manifestHostPermissionSet(manifest) {
  return Array.from(new Set([...(manifest.host_permissions || [])])).sort();
}

async function getActiveTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) {
    throw new Error("No active tab is available.");
  }
  return tab;
}

async function collectPageContext(tab) {
  try {
    return await chrome.tabs.sendMessage(tab.id, {
      type: "ONECONTEXT_COLLECT_PAGE_CONTEXT"
    });
  } catch (_error) {
    try {
      await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        files: ["content.js"]
      });
      return await chrome.tabs.sendMessage(tab.id, {
        type: "ONECONTEXT_COLLECT_PAGE_CONTEXT"
      });
    } catch (error) {
      return {
        url: tab.url || "",
        title: tab.title || "",
        selectedText: "",
        visibleText: "",
        domExcerpt: "",
        viewport: {},
        timestamp: new Date().toISOString(),
        collectionError: error.message
      };
    }
  }
}

function sendNativeMessage(payload) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendNativeMessage(HOST_NAME, payload, (response) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
        return;
      }
      resolve(response);
    });
  });
}

async function buildPayload() {
  const manifest = chrome.runtime.getManifest();
  const granted = await chrome.permissions.getAll();
  const tab = await getActiveTab();
  const pageContext = await collectPageContext(tab);
  const screenshotDataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, {
    format: "png"
  });

  const permissions = Array.from(
    new Set([...manifestPermissionSet(manifest), ...(granted.permissions || [])])
  ).sort();
  const hostPermissions = Array.from(
    new Set([...manifestHostPermissionSet(manifest), ...(granted.origins || [])])
  ).sort();

  return {
    type: "ONECONTEXT_BROWSER_EXTENSION_PROOF",
    userNote: document.getElementById("note").value,
    extension: {
      id: chrome.runtime.id,
      name: manifest.name,
      version: manifest.version,
      permissions,
      hostPermissions,
      permissionsApi: granted
    },
    tab: {
      id: tab.id,
      windowId: tab.windowId,
      url: tab.url,
      title: tab.title
    },
    pageContext,
    screenshotDataUrl
  };
}

document.getElementById("send").addEventListener("click", async () => {
  const button = document.getElementById("send");
  button.disabled = true;
  setStatus("Capturing current tab...");

  try {
    const payload = await buildPayload();
    setStatus("Sending native proof to 1Context...");
    const response = await sendNativeMessage(payload);
    setStatus(response);
  } catch (error) {
    setStatus(`Error:\n${error.message}`);
  } finally {
    button.disabled = false;
  }
});
