<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0056-registry-retained-checkpoints.md -->

# RFC 0056: Retained registry checkpoints and receipt verification

[日本語版](../ja/rfcs/0056-registry-retained-checkpoints.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E04, M5-F01
- Supersedes: none; elaborates RFCs 0045, 0046 and 0048
- Superseded by: none

## Summary

An explicitly opened host registry can export a bounded checkpoint after validating its complete committed history. A host that independently retains that checkpoint can later check its inclusion in the current history and verify an exported operation receipt at or before that checkpoint. Parsing a checkpoint does not authenticate it. These APIs grant no mutation, source-consumption or execution authority.

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST. The existing reachable-history proof supplies the necessary inclusion boundary without another transaction or storage namespace. An independent 22-check policy experiment covers rollback, forks, exact record association and receipt ordering. Python and JavaScript agree on a 339-byte canonical example and digest; the maximum valid field widths encode to 652 bytes. This proves policy/encoding only. Production integration, hostile-input behavior and both-host execution remain E04 acceptance conditions.

## Motivation

RFC 0045 R12/R16 requires independently retained evidence, and RFC 0046 explicitly leaves exportable verified receipts/checkpoints to E04/F01. `Receipt::as_bytes` exports an informational result, but no host API presently verifies retained receipt bytes or compares a retained head against the current complete history. The local hash chain alone cannot detect an operator replacing all history before a later observation. A prior independently retained checkpoint can expose a contradictory replacement without claiming operator honesty or global completeness.

## Goals

- Export an exact committed checkpoint and verify an independently retained checkpoint as an ancestor of the current validated head.
- Verify every existing receipt result variant against its complete committed audit record and a checkpoint's sequence bound.
- Combine unauthorized-operation, local recovery, restart and unchanged source execution with retained-evidence checks in the G5 exit journey.

## Non-goals

- Signature authentication, public checkpoint endpoints, transparency witnesses, global non-equivocation, trusted timestamps or automatic retention.
- Restoring missing storage, choosing a replacement HEAD, adopting orphan generations or changing any mutation/HTTP authority.
- Granting the sealed package-consumption evidence reserved for F01–F04, or changing the service memory budget.

## Guide-level design

The host opens `Registry` with its explicit directory capability. `Registry::checkpoint()` returns an opaque `Checkpoint`; `as_bytes()` exposes its canonical bytes for the host to retain independently of the registry. On a later open, `Checkpoint::admit(bytes, identity)` checks encoding and the independently configured service identity, and `Registry::verify_checkpoint(&checkpoint)` checks committed inclusion. The host must choose a trustworthy retained copy; bytes obtained alongside an untrusted replacement history are not an independent anchor.

`Registry::verify_receipt(&checkpoint, receipt_bytes)` returns the existing informational `Receipt` only after checking both the checkpoint and receipt against the current history. A receipt newer than the checkpoint refuses even when it is in the current history. Capture a later checkpoint explicitly if evidence through that event is required. Existing receipt exports and exact-request reconciliation remain unchanged.

## Reference semantics

### Checkpoint profile 1

The ordered, closed record is `{format:"lang-registry-checkpoint", schema:1, registry, origin, state_revision, time_floor, state_sha256, audit_sha256}`. Encoding uses the existing two-space pretty JSON plus exactly one final LF, at most 1,024 bytes and depth 16. Registry/origin use `ServiceIdentity` validation and must exactly match the independent identity. Revision is 1..=65,536; time is 0..=253402300799; both hashes are exactly 64 lowercase hex characters. Reject duplicate/unknown/missing fields, nulls, unsupported format/schema, reordered/noncanonical JSON, trailing input and oversized input before retaining decoded data. `Checkpoint` retains only its private record and bounded canonical bytes. It has no public field constructor and no public serde deserializer.

`checkpoint()` calls the complete read-only history validator while the registry's existing nonblocking exclusive lock is held. It captures the validated current state hash, revision, persisted time floor and last audit hash with the configured identity. It performs no cleanup, transaction, clock sampling, event creation or filesystem write. A time floor is a committed field, not a claim about current wall-clock freshness.

### Inclusion and receipt verification

Verification validates the entire current history using the existing bounded inventory, archive identity and exact-transition checks. The checkpoint's state hash must occur in that history's reachable state set. Read that immutable state through the existing digest/no-follow boundary and compare all checkpoint fields to it. A future/rolled-back checkpoint, orphan state, changed identity, wrong time/revision or a fork without the retained state refuses. An older genuine ancestor remains valid after later operations; it does not prove that later operations were independently observed.

Receipt input uses the existing closed `{schema:1, audit_sha256, result}` encoding and 2 KiB/depth-16 bound. Decode through a private record type, not a general public deserializer. Its audit hash must occur in the same validated history. Read and validate that audit, require its sequence at or before the checkpoint's state revision, and compare the entire exported receipt against the audit's result and hash. This includes initialization, publish, yank, quarantine, owner, revocation, principal, provision and recovery result variants. A substituted result, unknown variant/schema, same-revision fork, uncommitted candidate or later receipt refuses. Successful verification constructs an informational receipt and never invokes current bearer authorization or replays a mutation.

Validate bounded caller input before expensive history traversal. Do not return partially verified results. Operations retain the existing lock and storage/request checks. Errors never reset floors, prune history or alter HEAD. No filesystem path or credential secret is exported.

## Execution modes

This is a host registry API; source syntax, MIR, bytecode, native ABI, JIT, embedded callbacks and WebAssembly are unchanged. The combined fixture must reopen the same immutable archive and execute the original source in VM and native mode after the recovery sequence.

## ABI and compatibility

The checkpoint is a new independently versioned informational format. Existing HEAD, state, audit, receipt and HTTP schemas do not change. Unknown checkpoint schemas fail closed; no migration, history rewrite or downgrade bypass exists. The Rust API remains experimental. F01 may authenticate separately retained evidence through an explicit signed format; it must not infer authentication from this parser or reuse this checkpoint as a package trust receipt without an accepted contract.

## Diagnostics and tooling

Use existing structured `REG` errors: malformed input for encoding/schema/field errors, limit for byte/depth overflow, integrity for identity/inclusion/result contradictions, and existing state/I/O errors for invalid managed storage. Error text contains no supplied source, paths, credential material or record dumps. No editor, formatter, LSP, DAP or CLI flow changes are required.

## Performance and cost model

Verification pays one existing full-history validation, plus one bounded immutable state read and, for a receipt, one 2 KiB audit read. Reuse the validated reachable set; do not collect another vector of historical states or replay the full traversal separately for the checkpoint and receipt. Checkpoint encoding/retention is at most 1 KiB; receipt admission/encoding is at most 2 KiB. The standalone explicit-directory host API retains its existing unmetered status. Do not add this method to `MeteredRegistry` or HTTP without a separately audited workspace reservation. There is no service-envelope increase, benchmark speed claim or automatic background work.

## Security and trust boundaries

The retained copy is a caller-selected comparison anchor. Successful parsing or self-consistent current history cannot authenticate who supplied it. A compromised operator can withhold data or present divergent histories to hosts that never compare anchors. A checkpoint older than the divergence cannot reveal that divergence; if the independent copy is lost or replaced, its protection is lost. Verification detects contradiction with a supplied anchor, not completeness, freshness, global non-equivocation or malicious source. It supplies no filesystem/network/process/guest capability and cannot construct a consumed package.

## Alternatives

- **No change:** leaves R12's retained-evidence comparison unimplemented and exported receipts unchecked.
- **External file-inspection tool:** duplicates private format and complete transition validation, risking acceptance of orphan or invalid history. Reuse the store's validated boundary.
- **Signed public transparency log:** stronger externally attributable evidence, but requires independent trust, succession, witnesses and availability policy. F01–F04 own signed package trust; adding a log here would expand the selected outcome.
- **Compare only HEAD or revision:** rejects valid descendants or accepts forged same-revision states. Exact retained-state inclusion plus complete receipt comparison is the smallest useful relation.

## Implementation plan

After E08 publication, implement the bounded host API on E04's branch, then one combined HTTPS/local-recovery/retained-evidence/source-execution fixture. Add hostile anchor/receipt and no-side-effect boundaries, extend the existing fuzz target, run the frozen baseline and latest-head both-host CI, and audit G5 against E01–E08. E04 is re-estimated 1 → 3 pt before implementation; no additional delivery is introduced. E08 remains the sole doing row until publication.

## Test plan

Check an independent canonical vector; exact maximum field widths and byte/depth boundaries; duplicate/unknown/null/noncanonical fields; all receipt variants; genuine old/current checkpoints; future, fork, rolled-back, orphan and cross-origin cases; forged results and later receipts; unchanged storage on success/refusal; preserved immutable bytes and both-host VM/native execution after actual service stop/local recovery/restart. Existing mutation crash/concurrency suites remain required rather than adding artificial writes to this read-only API. A deterministic admission-mutation corpus and the existing registry fuzz target cover new input surfaces. G5 cannot close on the external policy model alone.

## Open questions

No unresolved contract blocks implementation. Actual source integration, maximal valid records, hostile storage/input checks and both-host validation remain unverified. Authentication and retention policy for public signed evidence remains F01 work; this RFC neither resolves nor silently bypasses it.

## Sources

- [RFC 0045](0045-registry-threat-model.md), last updated 2026-10-31: R12/R16 and limitations of local hash chains.
- [RFC 0046](0046-registry-protocol-and-recovery.md), last updated 2026-10-31: bounded audit/recovery and the E04/F01 retained-evidence requirement.
- [RFC 0048](0048-registry-state-and-publication.md), last updated 2026-10-31: complete reachable history and receipt encoding.
- [RFC 0055](0055-registry-local-administration.md), last updated 2026-10-31: local lifecycle and recovery receipt variants.
- [RFC 0001](0001-project-charter.md#experimental-rfc-decisions), last updated 2026-10-28; experimental delegation dated 2026-10-17 JST.

All are project decisions, checked against E08 implementation `16a8e80` on 2026-10-31 JST. The 22-check experiment is design evidence, not a production or platform claim. No external implementation claim requires web research for this elaboration.
