# Fuel Ops architecture

## Ownership and composition

Fuel Ops is a modular monolith. Each feature has a root directory containing a Rust crate (`src/lib.rs`) and its React feature (`frontend.tsx`). The shared Rust `CoreModule`, platform-specific `BasePhasedController`, and frontend `HandleSystem` have their own root directories. The server explicitly registers modules; React explicitly registers pages. No global service locator is used.

| Module               | Public contract                                                                               | Owned data                                                   |
| -------------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| CoreModule           | `register`, `order`, `initialize`, `shutdown`; `AppState`, `Transaction`, `Actor`             | schema migration ledger, transaction infrastructure          |
| BasePhasedController | initialize, current phase, transition, next, previous, reset, cancel, subscribe               | transient controller state                                   |
| HandleSystem         | `getHandle`, `getLockHandle`, `use`, `release`, scoped subscriptions                          | temporary frontend ownership                                 |
| AuthModule           | `public_routes`, `authenticate`; JWT/session endpoints                                        | refresh sessions, token rotation history, rate-limit buckets |
| PermissionModule     | `resolve`, `validate_assignments`; role endpoints                                             | roles and explicit permission catalog                        |
| StationModule        | `StationManager::configuration`, `nozzle`, `tank`; station endpoints                          | station/equipment configuration                              |
| EmployeeModule       | employee HTTP API                                                                             | user accounts and station/role assignments                   |
| ShiftModule          | `ShiftManager::submitted`, `approve`, `volume`; shift endpoints                               | shifts, historical equipment/prices, readings, settlements   |
| ReconciliationModule | reconciliation HTTP API; fixed-point `calculate`                                              | reconciliation results, investigations, corrections          |
| InventoryModule      | `InventoryManager::change`, `dispense`, `InventoryDataProvider::measure`; inventory endpoints | balances, measurements, immutable movements                  |
| DeliveryModule       | delivery HTTP API                                                                             | delivery documents, measurements, approvals                  |
| AlertModule          | alert HTTP API; `process_events` durable consumer                                             | operational issues and issue history                         |
| DashboardModule      | dashboard and export HTTP API                                                                 | aggregate read models; no authoritative business state       |
| AuditModule          | audit and scoped version-history HTTP API                                                     | audit records and prior record snapshots                     |

Composition uses trait objects only for heterogeneous module registration. Business calls use concrete, explicit public APIs. Removing a leaf module requires removing its registration and frontend page entry; required dependencies are checked before initialization. Removing a module with a background consumer also requires removing that consumer from server composition. Missing dependencies, cycles, and duplicate names fail startup.

## MongoDB model

Every domain record has an ID, organization ID, station ID, version, creation/update timestamps, and typed `data`. Repository access always applies organization and station scope. Station IDs come from active user assignments; no role name is an authorization shortcut.

| Collection             | Contents                                                                                                               |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| organizations          | Organization identity and fictional seed marker                                                                        |
| stations               | Stations with owned tanks, pumps, nozzles, fuel types, thresholds, and current price tiers                             |
| users                  | Employee account identity, Argon2 hash, active flag, station memberships, role assignments                             |
| roles                  | Configurable station-scoped permission sets                                                                            |
| shifts                 | Schedules, employee/nozzle assignments, historical equipment/pricing snapshots, readings, settlement records, handover |
| reconciliations        | Phased reconciliation results and discrepancy investigations                                                           |
| corrections            | Append-only financial and inventory corrections to finalized reconciliations                                           |
| deliveries             | Documents, declared/measured quantities, source, investigation, approval                                               |
| inventory_balances     | Current recorded balance per tank                                                                                      |
| inventory_movements    | Immutable measurement, delivery, and dispensing movements with resulting balances                                      |
| inventory_measurements | Measurement source, timestamp, direct volume, and operator note                                                        |
| alerts                 | Operational issues, severity, owner, state, resolution, and history                                                    |
| audit_logs             | Actor, operation, entity, and before/after versions                                                                    |
| record_history         | Previous record snapshots; user password hashes are excluded                                                           |
| refresh_sessions       | Revocable authentication sessions                                                                                      |
| refresh_tokens         | Hashed refresh secrets and rotation consumption state                                                                  |
| auth_attempts          | Hashed rate-limit keys with TTL expiration                                                                             |
| outbox                 | Durable typed event envelopes and per-consumer checkpoints                                                             |
| write_guards           | Per-organization transaction serialization                                                                             |
| schema_migrations      | Applied module schema/index version ledger                                                                             |
| worker_health          | Last background-consumer failure timestamp and status                                                                  |

Equipment, assignments, readings, settlements, and issue history are embedded owned aggregates rather than independently editable relational-style tables. This preserves one clear authority and atomic snapshots. The permission catalog is code-defined; roles and grants are data-defined. A future schema migration can extract an aggregate if measured access patterns justify it.

Module initialization applies idempotent indexes and records initial schema version 1. `cargo run -p fuel-ops -- --migrate` runs initialization and exits. Future schema versions require explicit transformations before increasing the ledger version. Indexes include scoped unique record IDs, unique account emails, role names, delivery documents, reconciliation-per-shift constraints, movement idempotency, version history uniqueness, and reporting access paths.

## Transactions, authorization, and concurrency

Protected requests validate the JWT, check session revocation/expiry, load the active user, validate the station, and resolve current role permissions from MongoDB. Mutations enforce their specific permission again in the feature handler. Updates use expected versions; stale writes fail with HTTP 409. Requests already in progress may finish after a permission change, but subsequent requests read the new grants immediately.

A transaction first updates the organization's write guard. It then reads the authoritative records, validates invariants, writes versioned records, records audit/version history, and appends domain events. Critical financial/inventory operations commit together with majority write concern and snapshot reads. Transactions are not silently retried as business commands. An uncertain commit is retried only as a commit operation; if uncertainty remains, clients must reload state before deciding whether to resubmit.

Shift creation checks interval overlap across the organization's employee assignments and across the selected station's nozzles. Opening checks active nozzle ownership and the last closed handover reading. Reconciliation completion approves the shift and deducts each nozzle's recorded volume from inventory in the same transaction. Delivery completion records incoming measured volume and updates inventory in one transaction. Tank capacity and nonnegative recorded balances are enforced.

No frontend transition grants business authority. Feature handlers authorize and validate inside the command path, use the reusable phase graph, and persist the authoritative phase with optimistic concurrency.

## Events

Feature events implement the small `DomainEvent` trait. An envelope includes a unique event ID, type, organization, station, entity, timestamp, and typed serialized payload. Events are inserted into `outbox` in the same transaction as the business state and audit record. Only committed events are published to the in-process broadcast bus.

The broadcast bus is a wake-up optimization, not durable delivery. `AlertModule::process_events` reads unprocessed outbox entries, inserts any resulting issue, and records its consumer checkpoint in a transaction. Reprocessing is safe because issue IDs are derived from event IDs. Timer wake-ups recover from missed broadcasts and restarts. Outbox entries are retained; production retention must respect audit/replay requirements.

The current consumer reacts to discrepancy and inventory events and scans overdue open shifts. Other issue categories are entered manually. Synchronous public manager calls remain appropriate for invariants that must commit together; they are not replaced with long event chains.

Frontend successful commands publish typed `DataChanged` events. The composition root invalidates server-state queries for that station. Query polling picks up changes from other sessions. Temporary locks remain local and are never authorization credentials.

## Phase controller

Rust and TypeScript implementations share conceptual operations and lifecycle rules. Phase identifiers are enums or string unions. An operation reserves the controller, checks graph/guards, exits the old phase, enters the new phase, then publishes a lifecycle event. Terminal phases clean up. Rejected concurrent transitions cannot interleave.

Failed entry cleans partial resources and attempts to restore the old phase. Failed restoration or exit faults the controller. Reset explicitly cleans before reinitializing. Cancellation releases owned resources. Business transaction rollback, rather than a transient controller instance, protects persistent state from partially completed operations.

The station setup UI demonstrates the frontend controller and phase-owned handles. Approval/submission locks use the same HandleSystem through shared hooks.

## Handle ownership

ContinueHere's source was inspected read-only. The frontend preserves unique identities, configure/use/release semantics, lifecycle validation, idempotent release, failed-use rollback, provider ownership, and disposal cleanup. TypeScript uses closures and object identity instead of Rust mutex references.

Each lock request owns a separate identity in a resource's active-owner set. Releasing one handle cannot release another. Scoped hooks release component-owned handles on unmount and abort associated requests. `try/finally` releases operation and navigation handles after success or failure. StrictMode cleanup does not permanently dispose a provider. Cross-tab refresh is coordinated with the browser Web Locks API where available.

Persistent theme/station preferences use ordinary state and local storage. Access tokens stay in memory; refresh tokens stay in HttpOnly cookies. UI resource locks do not replace backend authorization.

## Money, volume, and reporting

- Money is integer IRR; the UI never silently converts to toman.
- Fuel volume is integer milliliters. Inputs accept up to three liter decimal places and normalize Persian/Arabic digits before integer parsing.
- Meter rollover is explicit and limited to one verified rollover per shift.
- Each shift snapshots price tiers. Allocation volumes must sum to the metered volume.
- Settlement calculation uses an i128 intermediate and rounds half-up to whole IRR once per nozzle; totals use checked bounds.
- Finalized source readings are not overwritten. Authorized financial and inventory corrections are recorded separately. Financial totals report corrections separately; inventory corrections update the current balance and append an immutable movement.
- All timestamps are UTC epoch milliseconds. Reports display Asia/Tehran local dates and Persian month/year ranges. Custom `to` is exclusive.

Dashboard aggregation stays in MongoDB. Current inventory and current open issues are snapshots; dispensing, delivery counts, shift completion, and financial summaries use the selected reporting range. Equipment filters narrow applicable data: tank/fuel filters affect inventory and deliveries, nozzle/pump/shift filters attribute dispensing movements, and shift status filters apply to shifts and their reconciliation/dispensing records. Inventory snapshots remain tank-level rather than claiming nozzle-level physical inventory. Organization is fixed by authentication. Cross-station reports reauthorize every selected station.

Inventory history carries forward the last recorded tank balance on days without movement using MongoDB aggregation. The first known balance is the earliest available history; no earlier physical state is invented. Chart time buckets use Gregorian boundaries with Tehran timezone for MongoDB weekly/monthly/yearly grouping; Persian date ranges and labels are applied independently. Daily, weekly, monthly, and yearly units describe the grouping, not a claim of sensor precision.

## Security and validation boundaries

Passwords use Argon2. JWTs have fixed algorithm/issuer/audience, ten-minute expiry, and a session identifier. Refresh secrets are random, stored only as SHA-256 hashes, rotated on use, and revoked as a family when a consumed token is replayed. The seven-day session expiry is absolute. Logout revokes the session; account changes revoke all of that user's sessions. Origin checks protect cookie mutations; the browser uses SameSite Strict, HttpOnly cookies, with Secure enabled for HTTPS deployment.

Login throttles both account and actual peer IP with MongoDB-backed buckets. Payload size is bounded. Database internal errors and secrets are not returned. Audit history excludes password hashes. No debug logging is added.

Source checks cannot replace compilation, integration tests, or browser inspection. See README for the exact manual validation steps and remaining deployment boundaries.
