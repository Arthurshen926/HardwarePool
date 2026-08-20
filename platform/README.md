# Platform Adapters

Platform adapters connect the shared HardwarePool Runtime to operating-system facilities. They are replaceable mechanisms, not the source of Core semantics.

Each adapter must implement only the capabilities it can prove on the target platform. Unsupported projections must be rejected explicitly rather than silently emulated.

Current directories are design slots. No production platform adapter is included in the bootstrap archive.
