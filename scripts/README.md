# Scripts

- `validate_repository.py` — offline structural checks used by `xtask ci`.
- `bootstrap-windows.ps1` — read-only prerequisite and system inventory check.
- `collect-lab-inventory.ps1` — collects Windows/ADB lab metadata into `test-results/`; review before sharing.
- `new_agent_task.py` — creates a non-overwriting active plan from `docs/plans/TEMPLATE.md`.

No script in this directory installs drivers, changes boot configuration, enables test signing, runs Driver Verifier, or modifies Secure Boot/BitLocker.
