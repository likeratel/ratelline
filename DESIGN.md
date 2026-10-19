# Design

## Source of truth
Status: Active. Updated: 2026-10-06. Surfaces: native menu-bar panel, dashboard, service/account management, notification and display settings.

Evidence reviewed: [App](src/App.tsx), [Connections](src/components/Connections.tsx), [AccountPicker](src/components/AccountPicker.tsx), [AccountAliasEditor](src/components/AccountAliasEditor.tsx), [NotificationPreferences](src/components/NotificationPreferences.tsx), [PanelPreferences](src/components/PanelPreferences.tsx), [base tokens](src/styles/base.css), surface CSS, [README](README.ko.md), [historical review](docs/design-review-2026-09-30.md), Playwright scenarios and `artifacts/ui-consistency-1080.png`. Existing bundled provider logos are retained. No external mockup/pixel reference exists. The design skill's referenced installed `templates/AGENTS.md` was absent; repository instructions and the available skill govern this pass.

Initial baseline before the October passes: an always-expanded 20-service catalog pushed the next action below the fold; alias debounce lacked successful-save confirmation; two bulk actions competed; display scope was global only; new projects lacked an enabled notification rule. These findings have been addressed. The user explicitly requested a full usability pass and authorized implementation. Previous preference for an expanded catalog is superseded by this task's first-use clarity requirement.

## Brand
Quiet, precise, approachable. Trust comes from clear connection identity, truthful query status, explicit saves and reversible local actions. Keep dark neutral surfaces, recognizable service logos and restrained semantic color. Avoid decorative metrics, unexplained icon-only account actions, invented integration support and dense control walls.

## Product goals
A new user can connect a service, verify the account, select projects and understand which executions they will see and hear about. Returning users can inspect recent activity without navigating setup instructions. Scope and save results must be visible. Success signals: next action visible at 1080×780 and 720×520; no horizontal overflow at 360px; consistent 36px controls; every save/error is understandable; all required flows covered by behavior tests.

Non-goals: deployment mutation, remote infrastructure management, a hosted backend, or automatically installing/logging into third-party tools.

## Personas and jobs
Developers using one or several personal/work accounts; first-time users unfamiliar with Ratelline terminology; team members who need all project runs while limiting another service to their own. The tray is a quick status check; the dashboard is for investigation and configuration.

## Information architecture
Dashboard: title/refresh, compact activity summary, clearly labeled search/account/period filters, current display scope, recent and active executions. Large sections render 50 rows initially with Show more; search always covers the entire loaded history. With no selected projects, show the connection action immediately after the header and omit inactive filters/history. Healthy connection details and explanatory help follow the activity lists; actionable failures remain before them. Sidebar service filters and one Settings entry remain.

Dashboard totals, pending queries and connection health follow the selected service/account; count only monitored accounts. A service with no selected projects offers its own project-selection action. Management links preserve the filtered account unless an explicit recovery destination overrides it. Failed connections expand automatically with affected accounts first. Search/account filters with no matches produce one neutral explanation and a clear-filters action that preserves saved display choices and returns focus to search.

Settings: Connections, Notifications, Menu bar, General. Connections opens with a compact service selector/current service and an explicit catalog action. Keep supported services separate from guidance-only entries. Each selected service reads in order: service overview → account & authentication → monitored projects. First connections show concise setup guidance. Connected accounts use a compact service/account row, current status and recheck action, then project management. Do not repeat setup steps on every visit; keep authentication instructions in a disclosure. Project selection persists immediately and shows saved feedback. Release bulk controls as soon as local persistence completes; query newly selected projects in the background and do not query on deselection. Keep all/clear behavior scoped to the search results and account. Display scope can be set for a service or project beside those settings. Do not hide account identity behind a nickname.

## Design principles
Use progressive disclosure for uncommon choices, not for essential next actions. Pair controls with labels and short contextual guidance. Project scope overrides service scope, which overrides the default display scope. Label inherited values explicitly. Display filters and notification rules are independent. Show errors/unknown actors/partial queries honestly. Preserve existing saved choices during migration. One primary page scroll; no nested project scrollbars.

## Visual language
Reuse `src/styles/base.css` tokens: spacing 4/8/12/16/24/32px; 36px field/button height, 28px compact tray control, 8px control and 12px surface radius. Dashboard body 13px, control/help 12px minimum. Section gaps 24px; related controls 8–12px; field label gap 8px. Use subdued separators and flat surfaces. Status always includes words or icon semantics. Native selects use a consistent height/padding; options remain OS native. Motion only for pending state; reduced-motion supported.

## Components
Base CSS owns tokens. Reuse labeled field, button, icon-button, section header, notice, SettingFeedback. Add a reusable accessible Switch with checkbox semantics/role switch and a visible text label. Account alias editor has Save/Cancel, pending, saved and retry states; Enter saves, Escape cancels, blur does not silently save. Bulk selection uses one button toggling Select all/Clear all for current results. Scope controls use Default (effective value), My runs, All runs; project default means service setting. Notification master uses Switch; new monitored projects receive enabled My runs + All results rules without enabling the global notification switch or overwriting existing rules.

## Accessibility
Target WCAG AA contrast. Native labeled controls; visible focus; switch keyboard Space and programmatic checked state; non-color status feedback; live save/error announcements. Expanded catalog exposes expanded state, keyboard navigation and an accessible heading. Small-screen controls wrap rather than truncate critical labels. No hover-only essential instructions. Honor reduced motion.

## Responsive behavior
1080px desktop: readable content width and compact sidebar. 720px: wrap filter/management rows, keep service/account/next action visible. 360px: compact navigation, stacked labeled fields and project controls, full-width primary actions where needed. Tray stays intentionally dense at 420px. No horizontal scrolling or reduced font sizes to compensate.

## Interaction states
Connection: not checked, checking, connected (with identity), login required, unsupported, partial/failed query. Preserve stale usable data with clear status. A failed connection recheck appears beside account controls above the project list, explicitly labels the previous result, and cannot be overwritten by another service’s refresh timestamp. Recovery links open the exact failing service and account. Project empty states distinguish not loaded, no projects, search mismatch and nothing monitored. Alias Save confirms only after confirmed persistence; failure keeps draft and retry. Immediate settings show pending/error; rows retain user choices. Disabling notifications preserves project rules. Fresh projects default to My runs + All results; existing off/custom rules stay unchanged. Unknown actors cannot match My runs and explain how to choose All runs. Hidden execution/Undo behavior remains persistent and local-only.

Settings recovery errors are global, persistent and localized in the dashboard, settings and tray. Show that the original file is preserved, saving is locked, and repair plus restart is required; raw diagnostics belong in Details. Suppress first-use onboarding and disable mutation controls while keeping readable results, filters and refresh available. Never label this state as a GitHub connection failure.

Notification permission checks distinguish loading, failure and ready. Show failure and a read-only retry beside the permission control; requesting OS permission remains a separate action. Every project-selection save path, including retry, confirms success and starts background queries for selections only; clearing selections does not query providers.

## Query freshness and investigation

Show query health separately from an execution result. Health is per `(provider, connectionId, targetId)` and names checking, successful, partial, failed, stale, or not-yet-confirmed results. Use the oldest successful time only when every target in the visible summary has a success; never let another target's result make a failed or stale target look current. Provider-supplied activity `updatedAt` remains the observed execution time, distinct from the local query attempt and success times.

Dashboard status filters are temporary local navigation state. They count and filter executions in the displayed scope, but do not save to settings or alter display scope, notification rules, or provider data. Execution details open in a native dialog, initially focus a close control, and return focus to the invoking row when closed. They show only collected fields, query health, and the existing open/hide actions.

Keep long-running executions visible. Mark an active item that has run for at least 24 hours as a long-running item with its last observed state; do not infer failure or hide it because of elapsed time. Continue excluding Vercel `BLOCKED` entries under the existing provider rule.

The connection progress list distinguishes a locally saved project selection from a completed first remote query. Its tool/account rows report observed prerequisites and connection state; it must not present an unverified CLI, credential, or query as complete.

## Content voice
Plain Korean with complete English/Japanese/Simplified Chinese equivalents. Prefer service, account, project, display scope, notification scope; avoid inconsistent monitor/target names. Name what a button changes. Explain local CLI prerequisites at the point of connection, and why unsupported services cannot be queried. Labels must make sense without the developer explaining the app.

## Implementation constraints
Tauri 2 + React/TypeScript + existing CSS; no new UI framework. Preserve CLI account isolation, IPC validation, persisted settings compatibility and bounded read-only queries. Service/project display rules must agree in dashboard/history/tray counts; notification delivery stays independent. Test new rule defaults, persisted inheritance, cross-account observations, explicit alias save/cancel/failure, bulk toggle under search, keyboard/small-screen layouts. Inspect screenshots after implementation. Run project TS/Rust checks and independent reviewer; distinguish real authenticated provider checks from fixture/command-contract checks. No real secrets or API payloads in logs/test artifacts.

## Verification record
2026-10-02: TypeScript checking, unit tests (63), frontend build, browser suite (61) plus affected/new regressions (including 720×520 and repeated service navigation), Rust tests (213), rustfmt, Clippy and macOS app/DMG packaging passed. Independent review covered scope inheritance, existing notification preservation, alias retry, Fly's current payload model and actual screenshots at 1080/720/360 plus 720×520. Reviewer findings were fixed and rechecked. The packaged native app was relaunched; dashboard, service/project scope controls, the notification switch, and alias Save/Cancel were inspected without modifying the user's saved settings. Windows and the nine absent providers' authenticated requests remain outside verified evidence.

2026-10-04: Commercial-readiness QA addressed selection/refresh coupling, large-history grouping and rendering, compact first-use/management layouts, account-specific recovery, and truthful failure/update states. Independent review found no remaining P1/P2 issue in the reviewed changes. The final Apple Silicon app and installer were signed, notarized, stapled and checked in Gatekeeper; native controls and existing connected accounts were inspected after restart. Current measurements and verification boundaries are recorded in [the product QA report](docs/qa-2026-10-04.md).

2026-10-04 UI/UX follow-up: Reproduced and corrected scoped counts/empty setup, filtered-account management routing, collapsed failure recovery, no-results recovery, selection retries and notification permission failures. Screenshots at 1080/720/360px have no horizontal overflow; a service-specific setup action fits the 720×520 view. Keyboard filter reset returns focus to search. Independent review found no remaining P1/P2 in this pass. Real VoiceOver and Windows accessibility remain untested.

2026-10-06 evolution and UX: target-scoped query health, freshness summaries, local execution-status filters, execution details, long-running labels, connection progress, and coalesced refresh requests are implemented. Connection guidance runs account verification and focuses the appropriate project/lookup control; concrete authentication errors remain visible. Details survive a status transition that removes their original list section. Panel actions name their dashboard destination in all four locales. Native tray handling latches click intent, consumes cancelled releases, only unhides the application before a dashboard exists, and preserves actually visible, unminimized windows on reopen.

TypeScript checking, 73 unit tests, frontend build, rustfmt, 239 Rust tests, and Clippy passed. The frontend build has a 535.50 kB chunk-size advisory. The 118-case browser run passed 117 cases; the remaining assertion expected intentionally removed 24-hour cutoff copy. After updating it, all 16 affected dashboard/panel cases passed. Independent review found no remaining P1/P2 after fixes. The final isolated macOS build showed its initial panel, preserved an open dashboard on reacquisition, and restored a minimized dashboard only through its explicit panel button during inspection. Native status-item click and multi-monitor behavior remain unverified; precise observation limits are recorded in [the QA report](docs/qa-2026-10-06.md).

## Open questions
- [ ] Maintainer: real authenticated checks for services without local credentials/installed CLIs. Impact: cannot claim live support verification; publish exact evidence per service.
- [ ] Maintainer: Windows/native accessibility validation requires that platform. Impact: browser/macOS evidence is not Windows certification.
