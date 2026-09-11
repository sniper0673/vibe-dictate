# Smart Output

Smart Output keeps the existing recording and STT pipeline unchanged and routes only the final text delivery step.

## Routing

- Browser (`chrome.exe`, `msedge.exe`, `brave.exe`, `vivaldi.exe`): deliver through the browser extension into the currently active HTTP(S) tab.
- Terminal (`WindowsTerminal.exe`, `powershell.exe`, `pwsh.exe`, `cmd.exe`, `conhost.exe`): type Unicode text directly with `SendInput`; clipboard paste is not used.
- Other applications: retain the upstream clipboard + Ctrl+V behavior.

The target is detected after transcription completes, so the user may switch windows or Chrome tabs while speaking. Smart Output never activates a remembered browser tab.

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
