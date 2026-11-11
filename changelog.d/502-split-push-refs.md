### Code clean-ups

- Internal: a push is built from named steps (negotiation with lease check and `pre-push`, the
  remote's per-ref answer, the outcome) and takes its settings as one `PushRequest` instead of
  nine parameters (#502).
