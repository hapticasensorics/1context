# 1Context Browser Bridge Dev Preview

This is a dev-scoped Chrome extension preview for 1Context remembering. It is an
unpacked Manifest V3 extension plus a Chrome Native Messaging host, installed
manually for local permission and setup proof work.

The extension proves:

- Chrome extension id and version
- `nativeMessaging`, `scripting`, `storage`, and `tabs` permissions
- `<all_urls>` host permission
- current tab URL, title, selected text, visible text, DOM excerpt, and a
  screenshot evidence bundle

The native host writes `RememberingBrowserExtensionProof` into the installed
1Context app's preferences plist. The setup screen turns Browser Extension
Permissions green only after this native-message proof exists for the current
signed app identity. This extension is not a product release gate yet.

## Dev Install

```bash
./scripts/install-browser-extension-dev.sh
```

Then open `chrome://extensions`, enable Developer mode, click **Load unpacked**,
and choose:

```text
browser-extension/extension
```

The extension id is deterministic:

```text
ijkabgddnhgkapedaloabgpcmpdhdhpb
```

Click the extension's toolbar button, then **Prove + Capture Current Tab**.
