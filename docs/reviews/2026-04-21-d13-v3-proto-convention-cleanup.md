# D13 v3 — Proto Convention Cleanup Tracker

**Date**: 2026-04-21
**Status**: Tracker / deferred work
**Owner**: TBD (folded into v3 milestone)

## Why this doc exists

`api/proto/oneshim/dashboard/v1/dashboard.proto` (shipped via PR #455 V1 + PR #476 V2a) accumulated a few style drifts versus the project-wide proto conventions observed in `api/proto/oneshim/client/v1/*.proto` (consumer contract with the parent oneshim server).

These drifts were knowingly carried forward in V2b (streaming RPCs) — V2b's **new** messages adopt the full convention, but V2b does **not** retrofit V2a's already-shipped types, to keep V2b scope bounded.

This document captures the deferred cleanup so a future v3 pass can resolve it without rediscovering the context.

## Observed conventions (canonical)

Source of truth for project-wide proto style: `api/proto/oneshim/client/v1/*.proto` + Google's [Protobuf Style Guide](https://protobuf.dev/programming-guides/style/).

| Dimension | Convention | Example (from `client/v1/suggestion.proto`) |
|---|---|---|
| Timestamps | `google.protobuf.Timestamp` | `google.protobuf.Timestamp timestamp = 3;` |
| Empty messages | `google.protobuf.Empty` | `returns (google.protobuf.Empty);` |
| Enum zero value | `{ENUM_NAME}_UNSPECIFIED = 0;` | `PRIORITY_UNSPECIFIED = 0;` |
| Enum variants | `{ENUM_NAME}_VALUE` prefix | `HIGH` → `PRIORITY_HIGH` (when using full prefix) |
| Package layout | `oneshim.{scope}.v{n}` | `package oneshim.client.v1;` |

## V2a drifts (deferred for v3)

### D-1. `HealthCheckResponse.Status` enum naming

**Current** (v1, PR #455):
```proto
message HealthCheckResponse {
  enum Status {
    UNKNOWN = 0;
    SERVING = 1;
    NOT_SERVING = 2;
  }
  Status status = 1;
  string message = 2;
}
```

**Convention-aligned target**:
```proto
message HealthCheckResponse {
  enum Status {
    HEALTH_STATUS_UNSPECIFIED = 0;
    HEALTH_STATUS_SERVING = 1;
    HEALTH_STATUS_NOT_SERVING = 2;
  }
  Status status = 1;
  string message = 2;
}
```

- **Wire format impact**: **None**. Field numbers and enum integer values unchanged.
- **Source-level impact**: Rust enum variant names change (`Status::Unknown` → `Status::HealthStatusUnspecified`). Consumers in `crates/oneshim-web/src/grpc/mod.rs` + `crates/oneshim-web/tests/grpc_dashboard_integration.rs` need to update. Integration test assertions reference `HealthStatus::Serving as i32` — that still works after rename.
- **Blast radius**: 4-6 Rust call sites.

### D-2. String RFC 3339 timestamps (v2a fields)

**Current** (v2a, PR #476) — multiple fields carry string timestamps:
```proto
message RecentFramesResponse {
  ...
  message FrameMetadata {
    int64 frame_id = 1;
    string captured_at = 2;  // RFC 3339
    ...
  }
}

message ProductivityMetricsResponse {
  repeated MetricBucket buckets = 1;
  message MetricBucket {
    string start = 1;  // RFC 3339
    ...
  }
}
```

**Convention-aligned target**:
```proto
import "google/protobuf/timestamp.proto";

message RecentFramesResponse {
  message FrameMetadata {
    int64 frame_id = 1;
    google.protobuf.Timestamp captured_at = 2;
    ...
  }
}

message MetricBucket {
  google.protobuf.Timestamp start = 1;
  ...
}
```

- **Wire format impact**: **Breaking**. `string` vs `Timestamp` have different wire representations. Existing clients parsing the string will receive malformed data if the type flips directly.
- **Migration options**:
  - **(a) Deprecate + add new field** (recommended):
    ```proto
    message FrameMetadata {
      int64 frame_id = 1;
      string captured_at = 2 [deprecated = true];    // RFC 3339 (v2a compat)
      google.protobuf.Timestamp captured_at_ts = 6;  // v3
    }
    ```
    Server populates both for one release, then drops `captured_at` in v4.
  - **(b) Proto package version bump** (`oneshim.dashboard.v2`): clean slate, but every consumer must migrate simultaneously.
  - **(c) Straight replace**: breaks any shipped client.
- **Blast radius**: V2b streaming's `MetricBucket` + `DashboardEvent.occurred_at` already use `Timestamp` from the start, so v2b-era clients won't see the migration. Only v2a-era clients need the transition.

### D-3. Empty request messages

**Current**:
```proto
message GetAgentInfoRequest {}
message HealthCheckRequest {}
```

**Convention-aligned target**:
```proto
import "google/protobuf/empty.proto";

service DashboardService {
  rpc GetAgentInfo(google.protobuf.Empty) returns (AgentInfoResponse);
  rpc HealthCheck(google.protobuf.Empty) returns (HealthCheckResponse);
}
```

- **Wire format impact**: **None**. Empty message and `google.protobuf.Empty` serialize identically (zero bytes).
- **Source-level impact**: Rust call sites use `()` or `Empty {}` instead of typed empty messages. Minor.
- **Blast radius**: 2 RPCs × 1-2 call sites each.
- **Tradeoff**: Keeping typed empty messages is also valid — they leave room to add fields later without a breaking change. If we expect no fields will ever be added, `google.protobuf.Empty` is cleaner. Verdict for v3: **stay with typed empty messages** (preserves extensibility). Documented here only as a deliberate non-change.

## Summary table

| ID | Area | Wire breaking? | Migration path | Recommended v3 action |
|---|---|---|---|---|
| D-1 | Enum variant naming | No | Source-level rename | **Do** in v3 |
| D-2 | Timestamp types | Yes | Add-then-deprecate over 2 releases | **Do** in v3, start with (a) |
| D-3 | Empty messages | No | Not needed | **Skip** — keep typed for extensibility |

## Execution outline (when picked up)

1. Spec PR — update this doc with decision on D-2 migration path + target release numbers
2. Proto PR — update enum names (D-1) + introduce deprecated/new timestamp fields (D-2, option a)
3. Server PR — DashboardServiceImpl populates both old + new timestamp fields during transition window
4. Client PR — update external consumer guidance in `docs/guides/grpc-client.md` (if gRPC client guide covers dashboard)
5. Cleanup PR — after 1-2 releases, drop deprecated fields

Expected total effort: **~2 days** (spread across PRs for reviewability).

## Non-goals

- v3 is not a full proto redesign — scope stays surgical on D-1 + D-2(a).
- Unrelated convention work (e.g., Buf linter integration, proto registry) is out of scope for this tracker.

## Related

- [PR #455](https://github.com/pseudotop/oneshim-client/pull/455) — V1 proto foundation (introduced D-1)
- [PR #476](https://github.com/pseudotop/oneshim-client/pull/476) — V2a per-domain RPCs (introduced D-2)
- V2b streaming design (upcoming) — establishes the target convention for new messages; this tracker is its explicit deferred counterpart.
