# Smart Output

Smart Output keeps the existing recording and STT pipeline unchanged and routes only the final text delivery step.

## Routing

- Browser (`chrome.exe`, `msedge.exe`, `brave.exe`, `vivaldi.exe`): deliver through the browser extension into the currently active HTTP(S) tab.
- Terminal (`WindowsTerminal.exe`, `powershell.exe`, `pwsh.exe`, `cmd.exe`, `conhost.exe`): type Unicode text directly with `SendInput`; clipboard paste is not used.
- Claude desktop (`claude.exe`): restore the utterance-start window, then try to focus the known composer before clipboard delivery (cursor position is restored immediately after the focus click). If the focus probe is unavailable — foreground changed, window not maximized, too small — deliver at whatever already has focus instead of aborting; a submit is still attempted.
- OpenCode desktop (`OpenCode.exe`): restore the utterance-start window and deliver at whatever already has focus, same as an unrecognized app. An earlier `Ctrl+L` composer-focus probe was removed — it proved unreliable against real OpenCode window-activation timing and sidebar/Review-pane state, and silently "succeeded" even when it hadn't actually moved focus.
- Other applications: restore the utterance-start top-level window and retain the upstream clipboard + Ctrl+V behavior.

For PTT, the top-level delivery target is captured when the record button is pressed; for VAD, it is captured at `SpeechStart`. The user may move focus while speaking or while STT is processing: delivery temporarily returns to the captured window, then restores the newer foreground window afterwards. If the original window no longer exists or cannot be safely reactivated, delivery fails closed to clipboard copy instead of typing into the wrong place. Browser delivery still targets the active HTTP(S) tab inside the captured browser window, so changing tabs within that same browser window remains a separate edge case.

When no submit is requested (e.g. the local-control "finish without submit" command), Smart Output never runs any composer hunt or focus probe for any app, recognized or not — there is nothing for a hunt to protect since Enter is never sent, and probing (mouse move, shortcut injection) would only risk disturbing whatever the owner already has focused. Delivery in that case is always a plain paste (or, for a terminal, direct `SendInput` typing) at the current focus.

Browser delivery keeps a stricter fail-closed contract than desktop apps: if the extension cannot identify a confident input candidate on the page, the transcription is copied to the clipboard without pasting or submitting, because a browser page can hold many candidate controls and a blind paste risks landing in the wrong one. Claude/OpenCode desktop delivery does not carry this same page-full-of-controls risk, so their fallback is a plain in-place paste instead.

## Browser bridge

The Chrome/Edge extension uses Native Messaging host `com.brstk.vibe_dictate`. The native host is a second copy of the same binary named `vibe-dictate-native-host.exe`; it relays messages to the already-running tray process over loopback `127.0.0.1:47831`.

Extension ID is fixed by the public manifest key:

`jjlhamjlfcjmfcjhbokpjbjejendhnoj`

The native host manifest allows only that extension origin. The TCP listener binds loopback only.

## Input selection

The content script ranks visible `textarea`, `contenteditable`, `role=textbox`, and ordinary text inputs. It strongly prefers the currently focused editor, large controls in the lower part of the viewport, and message/prompt/chat-labelled controls. Search/find/filter/address controls are penalized.

If no candidate reaches the confidence threshold, the extension refuses insertion. Vibe Dictate then copies the transcription to the clipboard, does not send Enter, and reports a short tray error. This is intentionally fail-closed: no output is better than sending a command or message to the wrong control.

Browser insertion uses the native value setter plus an `input` event for inputs/textareas, and selection-aware `insertText` for contenteditable controls. The physical Enter remains a Windows `SendInput` keypress after the extension has focused the selected editor.

## Local installation

Build:

```powershell
.\scripts\build-windows-local.ps1 -Action test
.\scripts\build-windows-local.ps1 -Action release
```

Register Native Messaging and copy extension files into the runtime directory:

```powershell
.\scripts\install-browser-bridge.ps1
```

Unmanaged Chrome does not allow silent force-installation of a non-Web-Store extension. One manual step is therefore required: open `chrome://extensions`, enable Developer mode, choose **Load unpacked**, and select `%LOCALAPPDATA%\Programs\VibeDictate\browser-extension`. The fixed manifest key keeps the extension ID stable.

Set `[output] mode = "smart"`. Browser bridge failure does not fall back to Ctrl+V because that could paste into the wrong currently-focused browser control; it only preserves the result on the clipboard.
