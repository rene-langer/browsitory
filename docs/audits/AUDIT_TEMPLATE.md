# Browsitory Full-Audit Agent Prompt

Use this prompt to commission a fresh, evidence-based audit of the Browsitory
repository. Replace `{{AUDIT_DATE}}` with an ISO date (`YYYY-MM-DD`) before
issuing it.

---

You are the lead auditor for Browsitory, a Rust/Tauri desktop Git client with a
React/TypeScript frontend and a VS Code extension. Perform a full audit of the
repository as it exists at the audit commit. Your goal is to give project
leadership a trustworthy view of quality, risk, readiness, and the highest-value
remediation work.

## Audit principles

- Be evidence-led. For every finding, cite exact files and line numbers, the
  relevant test, command output, or a documented reproduction path.
- Separate **confirmed defects**, **credible risks**, **coverage gaps**, and
  **intentional trade-offs**. Do not call an unverified possibility a defect.
- Do not modify production code. You may run safe, read-only checks and the
  repository's documented build, lint, test, and static-analysis commands.
- Respect the repository's architecture and stated project phase. Identify
  incomplete roadmap work as a gap only when it conflicts with a documented
  acceptance criterion or a claimed capability.
- Reassess the scope from the current tree, architecture documentation, CI,
  manifests, test suites, and release assets. Do not assume this template's
  focus areas are exhaustive or that every listed integration is present.

## Phase 1: establish the audit scope

Before deep review, map the current implementation and identify which focus
areas materially apply. Record the evidence used to include, reduce, merge, or
omit a focus area in the executive summary's "Scope and method" section.

At minimum, consider these areas; audit each that exists or is materially
implied by code, configuration, tests, or release workflows:

1. Architecture and boundaries: crate/package ownership, the RepoClient IPC
   seam, Tauri command exposure, DTO consistency, frontend state boundaries,
   and desktop/extension parity.
2. Git-domain correctness and data safety: repository discovery, status, log,
   diffs, staging, commits, refs/branches, worktrees, conflicts, and behavior
   on malformed or unusual repositories.
3. Concurrency, lifecycle, and recovery: worker ownership, cancellation,
   repository switching/closing, process failures, error propagation, and UI
   consistency under concurrent actions.
4. Remote, forge, and credential security: authentication and secret handling,
   host validation, network error/retry behavior, provider isolation, privacy,
   and destructive remote operations.
5. Frontend quality: state transitions, loading/error/empty states, keyboard
   behavior, accessibility, rendering performance, and unsafe user-controlled
   content handling.
6. VS Code extension and transport bridge: protocol compatibility, process
   lifecycle, request correlation, error handling, configuration, and E2E
   coverage. Omit only if the extension is absent.
7. Verification strategy: unit, integration, frontend, desktop E2E, extension
   E2E, boundary-wire-format tests, fixtures, regression coverage, and the
   reliability of the test commands themselves.
8. Build, CI, release, and supply chain: reproducibility, platform packaging,
   versioning/changelog controls, dependency and license policy, lockfiles,
   permissions, and workflow protections.
9. Documentation and operability: architecture accuracy, developer setup,
   diagnostic paths, security guidance, release/runbook completeness, and
   consistency between roadmap claims and shipped behavior.

Add focused areas when the current repository exposes meaningful additional
risks (for example updater behavior, telemetry, data migrations, localization,
or accessibility tooling). Merge overlapping areas where a separate report
would duplicate evidence.

## Required audit method

1. Capture the commit SHA, working-tree state, supported platforms, and audit
   environment constraints.
2. Read the project architecture, AGENTS instructions, manifests, CI workflows,
   release scripts, and relevant specifications before judging implementation.
3. Trace each selected capability end-to-end across UI/extension, transport,
   application layer, Git/remote implementation, and tests.
4. Run the relevant documented checks where the environment permits. Report
   commands, outcomes, skips, and the reason each skipped check could not run.
5. For high-severity behavior, seek a second independent confirmation through a
   test, static trace, or a minimal safe reproduction.
6. Identify contradictions between documentation, type contracts, wire formats,
   runtime behavior, tests, and packaging/CI configuration.

## Deliverables

Write all output under `docs/audits/{{AUDIT_DATE}}/`. Create the directory if
needed; do not overwrite a prior audit. Use this exact structure:

```text
docs/audits/{{AUDIT_DATE}}/
  EXECUTIVE_SUMMARY.md
  deep-dives/
    01-<focus-area>.md
    02-<focus-area>.md
    ...
```

`EXECUTIVE_SUMMARY.md` must contain:

- audit identity: date, commit SHA, branch, working-tree state, auditor,
  environment, commands run, and limitations;
- scope and method, including which candidate focus areas were selected,
  merged, or omitted and why;
- an at-a-glance severity table (Critical, High, Medium, Low, Informational),
  with finding IDs that link to the supporting deep-dive;
- overall readiness assessment with explicit confidence and caveats;
- a prioritized remediation roadmap grouped into immediate, near-term, and
  strategic work; and
- positive controls and strengths worth preserving.

Create one deep-dive for each selected focus area. Each deep-dive must include:

1. Purpose, scope, and audit questions.
2. Architecture or execution path examined.
3. Evidence reviewed and checks run.
4. Findings, each using a stable ID such as `AUD-{{AUDIT_DATE}}-GIT-001`.
5. For every finding: severity, confidence, affected components, evidence,
   impact, realistic trigger/reproduction, recommended remediation, and
   suggested verification.
6. Coverage gaps, open questions, and accepted risks with no finding ID where
   appropriate.
7. Strengths and controls that reduced risk.

Use the following severity definitions consistently:

- **Critical:** likely loss of user data, credential compromise, arbitrary code
  execution, or an inability to safely use a core workflow.
- **High:** serious integrity, security, or availability failure with realistic
  impact but narrower prerequisites or blast radius.
- **Medium:** material correctness, recovery, maintainability, or coverage risk
  that should be scheduled.
- **Low:** bounded defect, polish issue, or hardening opportunity.
- **Informational:** observation with no demonstrated material risk.

## Completion standard

Do not finish with generic advice. The result must let a maintainer decide what
to fix first without rediscovering the evidence. If evidence is insufficient,
say exactly what could not be established and the smallest next action needed
to establish it.
