# Roadmap

## Update Checks

1Context uses Homebrew for upgrades. The app only checks whether a newer release exists and tells Homebrew to upgrade.

Current path:

- Check GitHub's latest-release redirect: `https://github.com/hapticasensorics/1context/releases/latest`
- Read the latest version from the redirected tag URL
- Store the last-seen version under `~/Library/Application Support/1Context/update/`
- Run a hardcoded Homebrew command when the user chooses to update

This avoids GitHub API rate limits and keeps the updater independent of the website while `haptica.ai` is changing quickly.

Future path:

- Move update metadata to a static JSON file we control, such as `https://haptica.ai/1context/latest.json` or a dedicated releases subdomain
- Keep the same local state file and Homebrew upgrade command
- Support stable/beta channels, minimum supported versions, security notices, and release notes

The client already accepts both the GitHub redirect shape and a simple JSON shape with `version` and `notes_url`, so moving from GitHub to hosted JSON should be seamless for people who update normally.

## Packaging

- macOS public preview ships as a Homebrew Cask.
- Apple Silicon and macOS 13 Ventura or newer are required.
- The cask installs the menu bar app, runtime, and `1context` CLI.
- `1contextd` remains internal implementation plumbing.

## Uninstall

Homebrew Cask only runs `zap` cleanup during the uninstall command that includes `--zap`. If a user runs a normal cask uninstall first, Homebrew no longer has the installed cask state needed for a later zap.

Future path:

- Add `1context uninstall` for friendly app/runtime removal.
- Add `1context uninstall --delete-data` for full local data cleanup.
- Internally map Homebrew installs to the right cask uninstall/zap flow so users do not have to remember Homebrew lifecycle details.

## Product Runtime

The public repo currently validates installation, menu-bar control, local runtime lifecycle, and update plumbing.

The deeper context engine, project memory, wiki generation, MCP surfaces, and capture flows are still in active development.
