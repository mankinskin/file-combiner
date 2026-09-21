Context

The new repository E2E policy at `workflow-tools/test/.agents/instructions/testing/end-to-end-testing.instructions.md` requires every checked-in E2E test to run against a real, fully wired backend. Network interception that returns synthetic application API responses (`page.route` or equivalent) and fake/stubbed backend processes are forbidden for checked-in E2E tests and scripts whose purpose is to run those tests.

Observed non-compliance

- `file-combiner/scripts/smoke-browser.sh` currently launches a fake backend process before running Playwright smoke tests.
- The conflict is recorded in `transcripts/09-09-2026_e2e-testing-policy/ARTIFACTS.md` and planned in `transcripts/09-09-2026_e2e-testing-policy/ROADMAP.md`.

Options for remediation

1. Wire the smoke script to use the canonical real backend test fixture.
   - Replace the fake backend start with the repository's supported test backend startup path, such as a docker-compose test fixture or the same startup path used by the integration harness.
   - Acceptance: the smoke run exercises the real backend endpoints; Playwright assertions that previously passed against the fake stub are verified against the real service; CI succeeds reproducibly.

2. Document and approve an explicit exception.
   - If the script is intentionally a fast developer-only check and must not be treated as checked-in E2E coverage, document the exception, move the script to a clearly labeled dev-only path or otherwise exclude it from CI E2E paths, and link the approval to this ticket.
   - Acceptance: the script is either out of CI E2E paths or has a documented owner-approved exception.

3. Replace the smoke script with a small harness that boots the canonical test backend under a reproducible fixture.
   - Acceptance: same as option 1, with the fixture committed and documented.

Non-goal

This ticket does not itself change `file-combiner/scripts/smoke-browser.sh`; this ticket records the remediation work and acceptance criteria.

Validation

- Run the smoke-browser E2E path after remediation and confirm the test run uses a real backend, not a fake/stubbed backend.
- Confirm the implemented path complies with `workflow-tools/test/.agents/instructions/testing/end-to-end-testing.instructions.md`.
