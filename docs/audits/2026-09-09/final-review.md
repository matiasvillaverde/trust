autoreview clean: no accepted/actionable findings reported
overall: patch is correct (0.98)
The flattening change correctly makes nested bracket legs fallback snapshots while allowing directly fetched top-level orders to win exact ID lookups. Removing the redundant fetch-time leg insertion preserves fallback behavior through sync-time flattening, and the channel-gated concurrency test deterministically exercises the intended claim boundary without introducing a production regression.
