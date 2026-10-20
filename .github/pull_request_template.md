<!-- Keep this checklist short enough to read and specific enough to matter. -->

## Summary

<!-- What changed and why. Link the issue: `Closes #123`. -->

## Checklist

- [ ] Linked to an issue (`Closes #...`)
- [ ] Tests cover the change (Rust and/or Vitest), or the reason they cannot is below
- [ ] UI strings exist in both `src/lib/i18n/en.json` and `src/lib/i18n/tr.json`
- [ ] `npm run docs:stats` and any RELEASE_READINESS claim updated when screens, commands or tests moved
- [ ] No generated file edited by hand (lockfiles, `src/lib/generated/`, `src-tauri/gen/`)
- [ ] Honest scope: limitations are documented and nothing unshipped is claimed
- [ ] Size: under ~400 lines of non-generated code, or the reason is below

## Size / risk notes

<!-- Large PRs: why this cannot be split further. Which gate proves the change (test name, E2E scenario)? -->
