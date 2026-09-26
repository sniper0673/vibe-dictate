const HOST = 'com.brstk.vibe_dictate';
let nativePort = null;
let reconnectTimer = null;
let profileFocused = false;

function scheduleReconnect() {
  if (!profileFocused || reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectNative();
  }, 3000);
}

function disconnectNative() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  const port = nativePort;
  nativePort = null;
  try { port?.disconnect(); } catch {}
}

function connectNative() {
  if (!profileFocused || nativePort) return;
  try {
    const port = chrome.runtime.connectNative(HOST);
    nativePort = port;
    port.onMessage.addListener(handleNativeMessage);
    port.onDisconnect.addListener(() => {
      // Native-host disconnects are recoverable here. Reading lastError marks
      // the callback error as handled so Chrome does not accumulate an
      // "Unchecked runtime.lastError" extension error before reconnecting.
      void chrome.runtime.lastError;
      if (nativePort === port) nativePort = null;
      scheduleReconnect();
    });
  } catch {
    nativePort = null;
    scheduleReconnect();
  }
}

function setProfileFocused(focused) {
  const next = Boolean(focused);
  if (profileFocused === next) {
    if (next) connectNative();
    return;
  }
  profileFocused = next;
  if (profileFocused) connectNative();
  else disconnectNative();
}

async function getActiveHttpTab() {
  const tabs = await chrome.tabs.query({ active: true });
  const httpTabs = tabs.filter(tab => tab.id && /^https?:/i.test(tab.url || ''));

  for (const tab of httpTabs) {
    try {
      const probe = await sendToPage(tab.id, { action: 'probe_focus' });
      if (probe?.ok && probe.has_focus) return tab;
    } catch {}
  }

  const fallback = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const tab = fallback[0];
  if (!tab?.id || !/^https?:/i.test(tab.url || '')) {
    throw new Error('unsupported_active_tab');
  }
  return tab;
}

async function refreshProfileFocus() {
  const tabs = await chrome.tabs.query({ active: true });
  for (const tab of tabs) {
    if (!tab.id || !/^https?:/i.test(tab.url || '')) continue;
    try {
      const probe = await sendToPage(tab.id, { action: 'probe_focus' });
      if (probe?.ok && probe.has_focus) {
        setProfileFocused(true);
        return;
      }
    } catch {}
  }
  setProfileFocused(false);
}

async function sendToPage(tabId, message) {
  try {
    return await chrome.tabs.sendMessage(tabId, message);
  } catch {
    await chrome.scripting.executeScript({ target: { tabId }, files: ['content-script.js'] });
    return await chrome.tabs.sendMessage(tabId, message);
  }
}

async function handleNativeMessage(message) {
  const id = Number(message?.id || 0);
  if (!nativePort || !id || message?.action !== 'deliver_text') return;
  try {
    const tab = await getActiveHttpTab();
    const result = await sendToPage(tab.id, { action: 'deliver_text', text: message.text });
    nativePort?.postMessage({ id, ...result });
  } catch (error) {
    nativePort?.postMessage({ id, ok: false, code: error?.message || 'browser_delivery_failed' });
  }
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.action === 'profile_focus') {
    setProfileFocused(Boolean(message.focused));
    sendResponse({ ok: true });
    return false;
  }
});

chrome.runtime.onInstalled.addListener(() => { void refreshProfileFocus(); });
chrome.runtime.onStartup.addListener(() => { void refreshProfileFocus(); });
chrome.windows.onFocusChanged.addListener(() => { void refreshProfileFocus(); });
chrome.tabs.onActivated.addListener(() => { void refreshProfileFocus(); });
chrome.alarms.onAlarm.addListener(alarm => {
  if (alarm.name === 'vibe-native-reconnect') void refreshProfileFocus();
});
chrome.alarms.create('vibe-native-reconnect', { periodInMinutes: 0.5 });
void refreshProfileFocus();