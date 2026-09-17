# Display streaming Adapter

Display-only engine ABI, authenticated launch records and bounded lifecycle
contracts imported from the display development worktree. The data plane stays
behind an explicit Adapter/process boundary; audio and input are not imported
from GameStream into this Adapter.

This source integration does not package the external native engine or prove
ordinary-user delivery. See `docs/adr/1422-integrate-display-with-the-unified-node.md`,
`docs/VIBEPOLLO_DISTRIBUTION_PLAN.md` and
`docs/MOONLIGHT_ANDROID_DISTRIBUTION_PLAN.md` for remaining distribution gates.
