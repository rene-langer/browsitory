# Browsitory Full Audit — 2026-09-05

This is the dated, ready-to-issue instance of the reusable
[`AUDIT_TEMPLATE.md`](../AUDIT_TEMPLATE.md). Give the following prompt to the
audit agent.

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
  manifests, test suites, and release assets. Do not assume the focus areas
  below are exhaustive or that every listed integration is present.

## Phase 1: establish the audit scope

Before deep review, map the current implementation and identify which focus
areas materially apply. Record the evidence used to include, reduce, merge, or
omit a focus area in the executive summary's "Scope and method" section.

At minimum, consider these areas; audit each that exists or is materially
implied by code, configuration, tests, or release workflows:

1. Architecture and boundaries.
2. Git-domain correctness and data safety.
3. Concurrency, lifecycle, and recovery.
4. Remote, forge, and credential security.
5. Frontend quality and accessibility.
6. VS Code extension and transport bridge.
7. Verification strategy and test reliability.
8. Build, CI, release, and supply chain.
9. Documentation and operability.

Add focused areas when the repository exposes meaningful additional risks, and
merge overlapping areas where separate reports would duplicate evidence.

## Required audit method

1. Capture the commit SHA, working-tree state, supported platforms, and audit
   environment constraints.
2. Read the project architecture, AGENTS instructions, manifests, CI workflows,
   release scripts, and relevant specifications before judging implementation.
3. Trace each selected capability end-to-end across UI/extension, transport,
   application layer, Git/remote implementation, and tests.
4. Run relevant documented checks where the environment permits. Report
   commands, outcomes, skips, and their reasons.
5. For high-severity behavior, seek a second independent confirmation through a
   test, static trace, or minimal safe reproduction.
6. Identify contradictions among documentation, type contracts, wire formats,
   runtime behavior, tests, and packaging/CI configuration.

## Deliverables

Write all output under `docs/audits/2026-09-05/`. Do not overwrite this audit
prompt. Create the results in this exact structure:

```text
docs/audits/2026-09-05/
  EXECUTIVE_SUMMARY.md
  deep-dives/
    01-<focus-area>.md
    02-<focus-area>.md
    ...
```

The executive summary must cover audit identity and limitations; scope and
method; a linked severity table; readiness assessment with confidence; a
prioritized immediate/near-term/strategic remediation roadmap; and controls or
strengths to preserve.

Write one deep-dive per selected focus area. Include the scope and audit
questions, paths examined, evidence and checks, stable finding IDs, and each
finding's severity, confidence, components, evidence, impact, trigger,
remediation, and verification. Also document coverage gaps, unanswered
questions, accepted risks, and positive controls.

Use these severity definitions consistently: Critical means likely user-data
loss, credential compromise, arbitrary code execution, or an unusable core
workflow; High means serious but narrower integrity, security, or availability
impact; Medium is a material risk to schedule; Low is a bounded defect or
hardening opportunity; Informational has no demonstrated material risk.

## Completion standard

Do not finish with generic advice. Give maintainers enough evidence to decide
what to fix first. For anything that cannot be established, state the exact
limitation and the smallest next action needed to resolve it.
