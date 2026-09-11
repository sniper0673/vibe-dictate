const HOST = 'com.brstk.vibe_dictate';
let nativePort = null;
let reconnectTimer = null;

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectNative();
  }, 3000);
}

function connectNative() {
  if (nativePort) return;
  try {
    const port = chrome.runtime.connectNative(HOST);
    nativePort = port;
    port.onMessage.addListener(handleNativeMessage);
    port.onDisconnect.addListener(() => {
      nativePort = null;
      scheduleReconnect();
    });
  } catch {
    nativePort = null;
    scheduleReconnect();
  }
}

async function getActiveHttpTab() {
  const tabs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const tab = tabs[0];
  if (!tab?.id || !/^https?:/i.test(tab.url || '')) {
    throw new Error('unsupported_active_tab');
  }
  return tab;
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

chrome.runtime.onInstalled.addListener(connectNative);
chrome.runtime.onStartup.addListener(connectNative);
chrome.alarms.onAlarm.addListener(alarm => {
  if (alarm.name === 'vibe-native-reconnect' && !nativePort) connectNative();
});
chrome.alarms.create('vibe-native-reconnect', { periodInMinutes: 0.5 });
connectNative();
