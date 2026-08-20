# Agent-first Development Workflow

## 1. Human and Agent roles

The human owner decides product scope, accepts architecture decisions, authorizes risky operations, performs physical-device interactions and evaluates user experience. Agents can inspect the repository, prepare plans, implement code, write tests, run allowed tools, analyze logs and prepare reviews.

Agent-first does not remove the software lifecycle. It makes specifications, automated checks and repository context more important.

## 2. Source of truth

In priority order:

1. explicit task acceptance criteria;
2. requirement IDs in `PRODUCT_REQUIREMENTS.md`;
3. architecture and Profile documents;
4. accepted ADRs;
5. tests;
6. existing implementation.

When two sources conflict, the Agent reports the conflict instead of silently reconciling it.

## 3. Task template

```text
Task ID: HP-CORE-001

Read first:
- AGENTS.md
- docs/PRODUCT_REQUIREMENTS.md (FR-CAP-001..006)
- docs/ARCHITECTURE.md sections 4-6

Goal:
Implement validated audio capability descriptors.

In scope:
- Core types and validation
- unit tests
- documentation comments

Out of scope:
- sockets
- Android/Windows APIs
- Tauri UI
- production cryptography

Acceptance criteria:
1. Microphone and speaker are independent capability instances.
2. Roles and supported projections are validated.
3. Unsupported audio formats return stable typed errors.
4. Tests cover success and failure paths.

Allowed dependencies:
- none without approval

Commands:
- cargo xtask fmt
- cargo xtask check
- cargo xtask test

Safety:
- no driver or device operations

Deliverables:
- changed files
- design explanation
- command output
- unresolved risks
```

## 4. Recommended Agent sequence

For a medium task:

1. **Planner** — reads specifications and writes a concise implementation plan.
2. **Implementer** — changes code and tests in a feature branch/worktree.
3. **Reviewer** — independently checks correctness, architecture, security and test gaps.
4. **Fixer** — addresses review findings.
5. **Hardware operator** — human authorizes and executes physical/privileged tests, with Agent assistance.
6. **Evidence summarizer** — records exact tested scope and remaining limitations.

One Agent can perform several roles, but the review pass should use a fresh context where possible.

## 5. Prompt hygiene

Good prompts specify behavior and evidence, not a preferred pile of code. Avoid requesting “finish the whole platform.” Prefer vertical slices that end in a measurable result.

Do not paste large transient logs into permanent instruction files. Store stable rules in docs, raw artifacts in test results, and task-specific facts in the issue/plan.

## 6. Plan files

Use `docs/plans/active/<id>.md` for work requiring multiple PRs or platforms. A plan tracks:

- objective;
- dependencies;
- milestones;
- acceptance evidence;
- decisions needed;
- risks;
- status and completed work.

Move it to `completed/` only when all exit criteria are met. Do not mark a hardware-dependent step complete based only on source generation.

## 7. Permissions

### Automatically allowed

- read repository files;
- edit documentation and ordinary source files;
- run formatter, linter, unit tests and non-privileged builds;
- create local test fixtures;
- inspect non-sensitive logs supplied for the task.

### Explicit approval required

- adding production dependencies;
- modifying public protocol compatibility;
- changing Android permissions or foreground-service declarations;
- installing APKs;
- changing driver INF/ACL/signing behavior;
- deploying or removing drivers;
- changing boot, Secure Boot, BitLocker or Verifier configuration;
- using credentials or publishing anything.

## 8. Completion report

Every Agent task should end with:

```text
Implemented:
- ...

Files changed:
- ...

Validation:
- command: result

Not validated:
- hardware/platform test and reason

Risks / next issue:
- ...
```

A statement such as “should work” is not evidence. A failed check is reported, not hidden.
