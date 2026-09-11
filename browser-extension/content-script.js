(() => {
  if (globalThis.__vibeDictateBridgeInstalled) return;
  globalThis.__vibeDictateBridgeInstalled = true;

  const SELECTOR = 'textarea,[contenteditable="true"],[role="textbox"],input:not([type]),input[type="text"]';
  const MIN_SCORE = 105;

  function visible(el) {
    if (!(el instanceof HTMLElement)) return false;
    if (el.matches(':disabled,[readonly],[aria-disabled="true"]')) return false;
    const style = getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return false;
    const r = el.getBoundingClientRect();
    return r.width >= 60 && r.height >= 18 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth;
  }

  function kind(el) {
    if (el instanceof HTMLTextAreaElement) return 'textarea';
    if (el instanceof HTMLInputElement) return 'input';
    if (el.isContentEditable) return 'contenteditable';
    return 'textbox';
  }

  function labelText(el) {
    return [
      el.getAttribute('aria-label'),
      el.getAttribute('placeholder'),
      el.getAttribute('data-placeholder'),
      el.getAttribute('name'),
    ].filter(Boolean).join(' ').toLowerCase();
  }

  function score(el) {
    if (!visible(el)) return -1;
    const r = el.getBoundingClientRect();
    let value = 0;
    if (el === document.activeElement) value += 160;
    if (el instanceof HTMLTextAreaElement) value += 110;
    else if (el.isContentEditable) value += 115;
    else if (el.getAttribute('role') === 'textbox') value += 95;
    else value += 70;
    if (r.width >= 300) value += 30;
    if (r.width >= innerWidth * 0.45) value += 20;
    if (r.top >= innerHeight * 0.45) value += 25;
    if (r.bottom >= innerHeight * 0.70) value += 30;
    const label = labelText(el);
    if (/(message|prompt|chat|ask|reply|send|type)/i.test(label)) value += 45;
    if (/(search|find|filter|url|address)/i.test(label)) value -= 180;
    return value;
  }

  function candidates() {
    const found = Array.from(document.querySelectorAll(SELECTOR));
    const active = document.activeElement;
    if (active instanceof HTMLElement && active.matches?.(SELECTOR) && !found.includes(active)) found.unshift(active);
    return found.map(el => ({ el, score: score(el) })).filter(x => x.score >= 0).sort((a, b) => b.score - a.score);
  }

  function insertText(el, text) {
    el.focus({ preventScroll: true });
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
      const current = el.value ?? '';
      const start = el.selectionStart ?? current.length;
      const end = el.selectionEnd ?? start;
      const next = current.slice(0, start) + text + current.slice(end);
      if (setter) setter.call(el, next); else el.value = next;
      const caret = start + text.length;
      try { el.setSelectionRange(caret, caret); } catch {}
      el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
      return;
    }

    const selection = getSelection();
    if (!selection) throw new Error('selection_unavailable');
    if (!el.contains(selection.anchorNode)) {
      const range = document.createRange();
      range.selectNodeContents(el);
      range.collapse(false);
      selection.removeAllRanges();
      selection.addRange(range);
    }
    if (!document.execCommand('insertText', false, text)) {
      const range = selection.rangeCount ? selection.getRangeAt(0) : document.createRange();
      range.deleteContents();
      const node = document.createTextNode(text);
      range.insertNode(node);
      range.setStartAfter(node);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
      el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
    }
  }

  function deliver(text) {
    if (typeof text !== 'string' || !text) return { ok: false, code: 'empty_text' };
    const ranked = candidates();
    if (!ranked.length || ranked[0].score < MIN_SCORE) {
      return { ok: false, code: 'no_confident_input_target', candidates: ranked.length, best_score: ranked[0]?.score ?? 0 };
    }
    const target = ranked[0];
    try {
      insertText(target.el, text);
      return { ok: true, target_kind: kind(target.el), score: target.score, candidates: ranked.length };
    } catch {
      return { ok: false, code: 'input_insertion_failed', score: target.score };
    }
  }

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || message.action !== 'deliver_text') return;
    sendResponse(deliver(message.text));
    return false;
  });
})();
