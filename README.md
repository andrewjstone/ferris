Ferris is a hierarchical timer wheel. There are multiple wheels with different resolutions to provide
a large time range with minimal memory use.

The wheel is optimized for large numbers of timers that are frequently cancelled instead of
expiring. This is useful for handling things like timeouts in distributed protocols. Note that
expiring timers are still very lightweight.

Start and Stop O(1) operations. Expiry is O(n) on the number of elements in the slot.

