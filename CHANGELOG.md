# Changelog

## 0.1.1 Alpha

- Rename the application to Read Atlas while preserving workspace, credential and preference identifiers.
- Pin the MSI upgrade code and use read-atlas.exe for distributed builds.
- Validate the per-user NSIS install, upgrade, uninstall and reinstall lifecycle with an isolated synthetic workspace.
- Fix Hub tags missing before Brief generation: editable labels now use persisted library tags rather than generated keywords.
- Publish English and Chinese guides with real desktop screenshots and official API-key links.

## Earlier Windows Alpha preparation

- Adopt MIT for original project code and documentation.
- Add Chinese and English entry points, setup instructions, privacy guidance, contribution guidance, and release records.
- Pin Node.js and Rust, add Windows CI and security checks, and prepare installer artifacts without automatic publication.
- Add a Chinese / English time and token notice before margin-note generation.
- Remove external Google Fonts requests and use local font fallbacks.
- Update PDF.js and Vitest to versions addressing the advisories found during preparation.
- Keep role presets and third-party character material unchanged in this work.

Earlier implementation history is preserved in [development history](docs/development-history.md) and [architecture decisions](docs/decisions.md). An entry here is not evidence of an installer release or completed live-provider acceptance.
