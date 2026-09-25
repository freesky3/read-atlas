# Dependabot review — 2026-09-25

Base: `3da61413628dd6e3314bfc047f454cb6e00fed7e` (`main`). Each original PR was reviewed against its own base and pinned head. The integration branch retains the original commits of the accepted candidates; it must pass current Windows CI and security checks before merging.

These PRs are weekly dependency updates, not reports that the application has fourteen bugs. A failed or cancelled workflow is evidence to investigate, not proof of an incompatible dependency.

## Individual decisions

| PR | Upgrade | Pinned head | Evidence and decision |
| --- | --- | --- | --- |
| [#1](https://github.com/freesky3/read-atlas/pull/1) | upload-artifact 4 → 7 | `02d59e0d95d60caca98c971c99d0d46dcc0a024e` | Include. Old CI run `34616935587` reached 563 passing Rust tests; its sole failure was the historical prompt hash/CRLF assertion. Installer artifact upload also requires a workflow dispatch on the integration head. |
| [#2](https://github.com/freesky3/read-atlas/pull/2) | webview2-com 0.38.2 → 0.39.1 | `7407c9efb04ffaa02fcbc816875e4460fcd3fd3c` | Hold. Run `34616947603` fails with E0277 in `webview_pinch.rs`: Windows COM interfaces and error types from 0.61 and 0.62 are incompatible. Tauri's platform controller and this direct dependency must be migrated together. |
| [#3](https://github.com/freesky3/read-atlas/pull/3) | setup-node 4 → 7 | `0d6725a8adc04badb3aaeb30b6a4ff3e2bf45ee1` | Include. Run `34616948175` installed the pinned Node version and passed frontend validation; only the old Rust CRLF assertion failed. |
| [#4](https://github.com/freesky3/read-atlas/pull/4) | checkout 4 → 7 | `418943a85749b1dda42afdd7cc6830a153df2f6d` | Include. Run `34616953536` completed checkout/build and failed only the old Rust CRLF assertion. |
| [#5](https://github.com/freesky3/read-atlas/pull/5) | sha2 0.10.9 → 0.11.0 | `91a5204394f9dbb8efd966f61e40705588f3c5bf` | Hold. Run `34616959350` fails with E0277: the new digest array lacks the `LowerHex` implementation used by existing `format!("{:x}", ...)` calls. Hash serialization needs an explicit migration preserving stored identities. |
| [#6](https://github.com/freesky3/read-atlas/pull/6) | keyring 4.1.6 → 4.2.0 | `c1d84f1493320e4c967c597e25607cd31c9d13cd` | Include. Run `34616977166`: 563 Rust tests passed, only the old CRLF assertion failed. Existing `v1` feature selection is retained. |
| [#7](https://github.com/freesky3/read-atlas/pull/7) | rusqlite 0.32.1 → 0.40.2 | `ce43830c7d7584f093cbde072f68117e6fb61335` | Include. Run `34616987500`: 563 Rust tests passed, only the old CRLF assertion failed. Database behavior is covered again by the integration suite. |
| [#8](https://github.com/freesky3/read-atlas/pull/8) | base64 0.22.1 → 0.23.1 | `5dc910776c6941e5b975b1cbb0a20ce6d9d76e29` | Include. Run `34616997626`: 563 Rust tests passed, only the old CRLF assertion failed. |
| [#9](https://github.com/freesky3/read-atlas/pull/9) | @xyflow/react 12.11.3 → 12.11.6 | `5699dae0e6cc57524a53a4815d7758e5057c3ca1` | Include. Run `34617002049` passed frontend checks/build; only the old Rust CRLF assertion failed. |
| [#10](https://github.com/freesky3/read-atlas/pull/10) | jsdom 26.1.0 → 30.0.1 | `adf3161e2214cbdff81012516633556272e3951a` | Hold. Run `34617019555` fails the BlockCardStream accessible button-name assertion. Reproduced locally against current main: jsdom 26 passes all 5 tests, jsdom 30 fails 1 and passes 4. This establishes test-environment incompatibility; it does not establish a production browser regression. |
| [#11](https://github.com/freesky3/read-atlas/pull/11) | plugin-dialog 2.7.2 → 2.7.3 | `b4645bc00514c433e576ac9785b5240675d43d24` | Include. Run `34617032677` passed frontend checks/build and was cancelled during Rust tests, not rejected by a failed assertion. |
| [#12](https://github.com/freesky3/read-atlas/pull/12) | user-event 14.6.4 → 14.6.7 | `a9e05c0215c4b4568f1688db5abacbb18a776683` | Include. Run `34617047666` passed frontend checks/build and was cancelled during Rust tests. |
| [#13](https://github.com/freesky3/read-atlas/pull/13) | vitest 4.1.11 → 5.0.0 | `2d9330e4f64ebec31e215579a13edf4c2ac85ad0` | Include. Run `34617070731` passed frontend checks/build and was cancelled during Rust tests. The complete frontend suite is required again on the integration head. |
| [#14](https://github.com/freesky3/read-atlas/pull/14) | cache 4 → 6 | `fa0edb5f45110a9461591d690705fe897c6b0bb4` | Include. Both checks passed on the corrected base. Current audit is still required because new advisories appeared after that run. |

## Shared baseline and integration fixes

- Main commit `3da6141` already fixes the historical prompt hash check for LF and CRLF. That fix is included without weakening the assertion.
- Today's audit found [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html) in main's rustls 0.23.43. The integration changes only rustls's locked version/checksum to 0.23.45. No audit exclusions or severity thresholds are added.
- `npm run notices` regenerates the checked-in notices for the final installed dependency graph (634 dependencies). Missing notices are not waived for automated PRs.
- Adjacent-line conflicts in workflow versions and npm manifests retain both intended upgrades. No application source, persisted data format, branch protection, or test expectation is changed.

## Validation gate

Run `npm ci`, `npm run check:repo`, `npm test`, `npm run build`, `cargo test --manifest-path src-tauri/Cargo.toml --locked`, `npm run notices`, `npm audit`, and `cargo audit --file src-tauri/Cargo.lock`. Also dispatch the Windows installer workflow to exercise upload-artifact 7 with both NSIS and MSI outputs. Current results belong to the integration PR and its linked workflow runs.

The local audit uses advisory database commit `593df8c1b5ed0bcde9dddadfeeead776fa514ff8`, verified against the upstream branch on 2026-09-25. Both npm and Cargo report zero blocking vulnerabilities after the rustls patch. Cargo retains pre-existing informational warnings; zero vulnerabilities does not mean zero warnings.

PRs #2, #5, and #10 remain open for separate compatibility work. Their failing tests are not removed or relaxed to include them in this batch.
