# Browsitory UI/UX Audit — 2026-09-20

This is the dated instance of the UI/UX audit brief given to the audit agent. It is a companion to
the engineering audit of 2026-09-05 (`../2026-09-05/`) and follows the same deliverable shape
(executive summary plus deep-dives, stable finding IDs, explicit severity and confidence).

---

You are a senior product designer, UX researcher, interaction designer, accessibility specialist,
and visual design systems expert.

Perform a comprehensive UI/UX audit of this application, evaluating it both:

1. **Visually:** how the interface looks, feels, and communicates hierarchy and quality.
2. **Logically:** how well the interface supports users in understanding, navigating, and
   completing tasks.

Do not treat this as a superficial visual critique. Explore the application as a real user would,
understand the underlying product logic, inspect the implementation where useful, and evaluate
whether the interface is coherent, predictable, accessible, efficient, and visually polished. The
goal is to identify specific, actionable improvements, not generic design advice.

## 1. Audit methodology

Understand the application before judging individual screens. Identify its apparent purpose,
primary user types, core user journeys, most important tasks, information architecture, major
navigation structures, major entities and concepts, recurring UI patterns, and design system (if
one exists).

Then explore systematically. Where possible: run the application; interact with every major
screen; follow complete user journeys; test navigation; resize the viewport; test keyboard
interaction; trigger errors; submit forms; inspect empty and loading states; test destructive
actions; inspect modals, menus, dropdowns, tooltips, dialogs, notifications and overlays; observe
transitions and animations; inspect mobile and desktop layouts; and inspect the implementation
when visual behavior cannot be understood from the UI alone. Do not evaluate only the happy path.
Actively search for situations where the interface breaks down.

## 2. Product and UX logic

For every major workflow determine: what the user is trying to accomplish; whether that goal is
obvious; whether the interface communicates the next action; whether the user understands what
will happen before acting; whether the workflow has unnecessary steps or hidden dependencies;
whether important actions are hard to discover; whether irrelevant options distract; whether the
system behaves consistently with expectations; whether users can recover from mistakes; whether
context is preserved; and whether the user knows what happened after an action.

Pay attention to mental models: terminology that conflicts with user expectations; navigation that
does not match the conceptual structure; controls that behave differently from how they look; the
same concept with multiple names; the same action represented differently in different places;
behavior that is technically correct but hard to understand.

## 3. Information architecture

- **Navigation:** understandable primary navigation, meaningful labels, obvious current location,
  logical grouping, deeply nested or buried destinations, unnecessary items, unexpected changes
  between screens, and whether mobile navigation preserves the conceptual structure.
- **Hierarchy:** can users quickly distinguish primary information, secondary information,
  metadata, actions, warnings, optional information and system status? Identify screens where
  everything has approximately the same visual importance.
- **Findability:** can users discover important features, settings, filters, search, secondary
  actions, help, account controls, destructive actions and recovery options?

## 4. User flows

Audit major workflows end to end, documenting: Goal → Entry point → Steps → Decisions → Actions →
Feedback → Completion → Recovery. Look for unnecessary steps, unclear transitions, redundant or
missing confirmation, ambiguous actions, unexpected navigation, loss of context, unnecessary data
entry, repetitive work, poor defaults, premature decisions, unclear completion, inability to undo,
dead ends and error loops. Pay particular attention to creating, editing, deleting, searching,
filtering, sorting, importing, exporting, onboarding, authentication, payments, settings,
permissions, collaboration and multi-step processes.

## 5. Visual design audit

Evaluate systematically rather than judging whether screens "look nice":

- **Layout:** spacing, alignment, margins, padding, container widths, grid consistency, vertical
  rhythm, density, whitespace, section separation, balance. Identify inconsistent spacing patterns
  and arbitrary layout decisions.
- **Typography:** font selection, type scale, hierarchy, line height, letter spacing, text
  density, readability, paragraph width, heading consistency, capitalization, numerical
  typography.
- **Color:** primary/secondary colors, semantic colors, contrast, hierarchy, muted text, disabled
  states, links, warnings, errors, success states, dark/light themes. Check whether color is the
  sole mechanism for communicating information.
- **Components:** buttons, inputs, selects, checkboxes, radios, tabs, cards, tables, navigation,
  badges, alerts, modals, drawers, tooltips, dropdowns, menus, pagination, date pickers, search
  fields. Look for inconsistent styling, behavior, variants, sizing, states and terminology, and
  for visual drift.

## 6. Design system consistency

Determine whether the application has a coherent visual language. Identify spacing, typography,
color, radius, shadow, iconography, interaction-state, breakpoint and layout tokens and
conventions. Flag one-off component styles, arbitrary spacing, inconsistent radii, button heights,
icon sizes and text styles, multiple versions of the same component, and visually similar
components with different behavior. If a design system exists in code, compare the implemented UI
against it.

## 7. Interaction design

Audit every major interactive element for hover, focus, active, pressed, disabled, loading, error,
success and empty states. Ask whether users can understand what is interactive, what is selected,
what is currently happening, what just happened, and what they can do next. Identify interactions
that depend on subtle visual cues without sufficient feedback.

## 8. Forms

Inspect labels, placeholders, helper text, required fields, validation, inline errors, error
placement and wording, field grouping, input types, autocomplete, defaults, formatting, character
limits, submission behavior, loading states, duplicate-submission prevention and preservation of
entered data. Evaluate whether validation happens at the appropriate time.

## 9. Feedback and system status

Evaluate loading indicators, skeletons, progress, toasts, inline feedback, success states, errors,
retries, background operations, autosave, synchronization, network failures and stale data. For
every significant action determine whether the user receives appropriate feedback. Look
specifically for actions that appear to do nothing, feedback that disappears too quickly or is too
vague, feedback far from the relevant action, operations that can fail silently, and success
messages that do not explain what changed.

## 10. Empty, loading, error, and edge states

For every major screen inspect or reason about the empty state (why it is empty, next action,
whether it teaches), loading state (visible, appropriate, layout jumps, premature interaction),
error state (understandable, data saved?, recovery, exposed technical detail), partial state
(partial load or partial failure) and first-use state (can a new user proceed without prior
knowledge?).

## 11. Accessibility

Serious review, evaluated against WCAG 2.2 AA where applicable: keyboard navigation, tab order,
focus visibility, focus trapping, focus restoration, semantic HTML, heading hierarchy, form
labels, accessible names, screen-reader behavior, ARIA usage, color contrast, non-color
indicators, touch target sizes, zoom, reduced motion, dynamic content announcements, and modal,
menu and tooltip accessibility. Distinguish confirmed problems, probable problems, and issues
requiring assistive-technology testing.

## 12. Responsive design and 13. Mobile UX

Review desktop, laptop, tablet and mobile at common widths: broken layouts, horizontal scrolling,
clipped content, oversized components, tiny controls, awkward wrapping, broken tables, modal
overflow, navigation problems, inconsistent spacing, desktop assumptions, touch problems. Ask
whether the interaction model remains appropriate at each size. Treat mobile as its own
experience: thumb reach, navigation, bottom actions, sticky controls, keyboard interactions,
scrolling, modal behavior, form completion, touch targets, gestures, content prioritization and
one-handed use.

## 14. Content and UX writing

Audit labels, buttons, headings, helper text, errors, confirmations, empty states, onboarding,
tooltips, notifications and system status for vague language, jargon, inconsistent terminology,
ambiguous CTAs, passive language, redundancy, over-long explanations, unclear errors and
inconsistent capitalization. Produce a list of important terminology inconsistencies.

## 15. Visual hierarchy and cognitive load

For each important screen: what is noticed first, is it the right thing, what second, is the
primary action obvious, can the screen be understood in a few seconds, is there too much
information, is it grouped logically, are there unnecessary decisions, must users remember
information between screens? Identify excessive density or whitespace, competing CTAs, visual
noise, weak hierarchy, excessive decoration, insufficient context and unnecessary cognitive load.

## 16. Consistency and predictability

Compare similar screens and workflows for differences in terminology, component behavior, spacing,
typography, navigation, button placement, confirmation behavior, error handling, loading behavior,
modal behavior and interaction patterns. Identify where consistency would improve usability and
where deliberate differentiation is justified.

## 17. Performance as perceived UX

Inspect initial load, route transitions, interaction latency, large content rendering, images,
animations, loading states, layout shifts and network-dependent interactions. Where possible,
connect the perceived problem to the technical cause.

## 18. Trust and perceived quality

Inspect destructive, irreversible, permission, financial, privacy and account actions, system
status, synchronization and saved/unsaved state. Identify moments where users could wonder "Did
that actually save?", "Did I just delete this?", "Who can see this?", "What happens if I click
this?", "Why did the screen change?" or "Can I undo that?"

## 19. UI implementation review

Where useful, inspect the frontend implementation for duplicated UI logic, inconsistent component
implementations, hard-coded design values, missing tokens, hard-to-reuse components, inconsistent
breakpoints, inaccessible custom controls, state-management problems affecting UI, layout hacks,
CSS specificity problems, excessive overrides and drifted variants. Do not recommend rewrites for
architectural neatness; connect implementation issues to observable UX consequences.

## 20. Screen-by-screen audit

For every significant screen produce: Screen; Primary user goal; First impression; UX clarity;
Visual hierarchy; Navigation; Interaction design; Accessibility; Responsive behavior;
Content/UX writing; Major issues; Recommended improvements. Include screenshots or visual
references where the environment allows. When referencing a visual issue, describe exactly where
it occurs; avoid vague comments such as "the hierarchy could be better."

## 21. User journey audit

For the most important flows produce: Entry point; User goal; Key steps; Friction points;
Confusing decisions; Error/recovery points; Completion feedback; Accessibility concerns; Mobile
concerns; Recommended changes. Identify where friction accumulates across multiple screens even
when each screen appears acceptable.

## 22. Heuristic evaluation

Evaluate against visibility of system status, match between system and real world, user control
and freedom, consistency and standards, error prevention, recognition over recall, flexibility and
efficiency, minimalist design, error recovery, and help and documentation. Tie each issue to an
actual screen, flow or interaction; do not merely list violations.

## 23. Findings format

Every substantive finding must include: **ID** (stable, e.g. `AUD-2026-09-20-A11Y-001`);
**Category** (UX / Visual / Accessibility / Responsive / Content / Interaction / Design System /
Performance); **Severity**; **Confidence**; **Location** (screen, route, component or file);
**Problem**; **Observed behavior**; **Why it matters**; **Recommendation**; **Implementation
notes** when useful. Avoid subjective criticism that cannot be tied to usability, accessibility,
consistency, hierarchy, comprehension or another defensible design principle.

## 24. Severity guidelines

- **Critical:** prevents users from completing a core task, causes a severe accessibility failure,
  creates serious risk of destructive mistakes, or makes a fundamental workflow unusable.
- **High:** significant friction or confusion in an important workflow, major accessibility
  problem, major responsive failure, or substantial visual/interaction inconsistency.
- **Medium:** meaningful usability or visual issue that does not fundamentally block the workflow.
- **Low:** minor inconsistency, polish issue, or localized friction.
- **Informational:** observation or improvement opportunity with limited immediate impact.

Do not inflate severity to make the report look impressive.

## 25. Prioritized recommendations

After the audit produce: top UX problems; top visual problems; top accessibility problems; top
mobile/responsive problems; quick wins; and systemic improvements (design tokens, component
standardization, navigation restructuring, form patterns, feedback patterns, accessibility
primitives, responsive conventions, content guidelines). Prioritize systemic fixes when they
address repeated problems.

## 26. Final deliverable

Finish with a concise executive summary containing: what works; what hurts the experience; what
hurts visual quality; what creates accessibility risk; and what should change first, as a
prioritized action list where each recommendation states the problem addressed, expected user
benefit, relative implementation effort, affected screens/components, and whether it is a local or
systemic fix. Also provide design system recommendations, a UX debt map (navigation, forms,
feedback, hierarchy, accessibility, responsive design, content, consistency, interaction
patterns), and audit limitations: explicitly list anything that could not be verified
(inaccessible screens, missing states, unavailable backend behavior, missing mobile environment,
unavailable assistive technology, unavailable analytics, untestable workflows). Do not invent
findings for behavior that could not be observed.

## Completion standard

The audit must be specific, evidence-based, visually informed and actionable. Avoid generic
statements such as "improve UX", "make it more modern" or "use better spacing." Every significant
criticism must answer: What is wrong? Where is it? Why does it matter? What should change?

## Deliverables

Write all output under `docs/audits/2026-09-20/`. Do not overwrite a prior audit or this prompt.
Use this structure:

```text
docs/audits/2026-09-20/
  AUDIT_PROMPT.md
  EXECUTIVE_SUMMARY.md
  deep-dives/
    01-<focus-area>.md
    02-<focus-area>.md
    ...
```
