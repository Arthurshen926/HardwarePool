# Testkit-specific Agent Rules

- Fixtures are deterministic and must not read host hardware or environment state.
- Production crates must never depend on testkit.
- Demo authorization is synthetic and must be labeled as such.
- IDs used by snapshots should remain stable unless fixtures and examples are deliberately versioned.
