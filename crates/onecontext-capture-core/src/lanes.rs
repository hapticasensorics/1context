pub const CONTRACT_VERSION: &str = "capture-window-bundle.v0";

pub fn mandatory_lane_ids() -> &'static [&'static str] {
    &[
        "capture.windows",
        "capture.displays",
        "capture.events",
        "capture.ax",
        "capture.ux",
        "capture.active_window_frames",
        "capture.browser",
        "capture.terminal",
        "capture.editor",
        "capture.external_refs",
    ]
}

pub fn required_bundle_files() -> &'static [&'static str] {
    &[
        "manifest.json",
        "sources.json",
        "time_alignment.json",
        "capabilities/capture.status.json",
        "capabilities/permissions.json",
        "capabilities/ux-event-tap.json",
        "capabilities/samplers.json",
        "capabilities/browser-extension-proof.json",
        "quality/known_gaps.jsonl",
        "events/windows.jsonl",
        "events/displays.jsonl",
        "events/capture.events.jsonl",
        "events/ax.events.jsonl",
        "events/ux.events.jsonl",
        "events/sck-frame-metadata.events.jsonl",
        "events/browser.events.jsonl",
        "events/terminal.events.jsonl",
        "events/editor.events.jsonl",
        "media/media.index.jsonl",
        "media/frames-2fps/",
        "external_refs/source-envelopes.jsonl",
    ]
}
