# Fuel Ops

Fuel Ops is a modular station operations application for privately operated fuel stations in Iran. The application uses Rust, Axum, MongoDB, React, TypeScript, and a Persian RTL interface. Business rules and permissions run on the server.

Every feature owns a directory at the project root. `server` composes the backend modules; `web` composes the frontend modules. There is no Flutter implementation or equipment control.

## Validation status

The implementation has received source inspection, Rust and TypeScript syntax/format checks, and configuration parsing. Compilation, dependency installation, automated tests, live MongoDB verification, and browser visual verification must be run manually. Do not treat this as a production deployment approval. The manual commands below are the handoff checklist; report their output before deployment.

## Prerequisites

- Ubuntu Linux with a current stable Rust toolchain.
- Node.js 22.12 or newer and npm.
- Docker Engine and the Docker Compose plugin.
- A MongoDB replica set. The local Compose file starts a single-node MongoDB 7.0.43 replica set and binds its port only to loopback.

## Local setup

Run commands from the repository root unless a directory is specified.

1. Copy `.env.example` to `.env`.
2. Set `JWT_SECRET` to at least 48 random characters, for example the output of `openssl rand -hex 48`.
3. Set `DEMO_PASSWORD` to a unique password containing at least 12 characters. No default working password is committed.
4. Start MongoDB with `docker compose up -d`. Wait for `docker compose ps` to show a healthy service.
5. Install frontend dependencies with `npm --prefix web install`.
6. Apply the initial index migrations with `cargo run -p fuel-ops -- --migrate`.
7. Populate the empty database with `cargo run -p fuel-ops --bin seed`.
8. Start the API with `cargo run -p fuel-ops`.
9. In another terminal, run `npm --prefix web run dev`.
10. Open http://localhost:5173. Keep this URL consistent with `WEB_ORIGIN`.

The API listens on `127.0.0.1:8080` by default. Vite proxies `/api` and `/health`, so the browser uses a same-origin API. The MongoDB health check initiates the replica set automatically; there is no separate initialization container. The API is intended to run on the host with this Compose configuration. A containerized API needs a replica-set hostname reachable from its network.

The seed command refuses to overwrite an existing user database. Once its seed marker is complete, rerunning it does nothing. All seed data, names, stations, fuel prices, settlements, and operational events are fictional. Seed prices are examples, not statements of current regulated prices.

## Development launcher

After completing the one-time local setup above, run `./dev.sh` from the project root as your normal user. Do not use `sudo ./dev.sh`; the launcher refuses to run as root. The script also works when invoked by its path from another directory. It requires Bash, `setsid` (provided by Ubuntu's `util-linux` package), Cargo, npm, and a Docker Compose plugin supporting `up --wait` and `--wait-timeout`. Docker must be running, and frontend dependencies must already be installed.

The launcher checks Docker daemon access first. If direct access works, it does not use sudo. Otherwise, it requests sudo authentication once at startup and uses `sudo docker` for its Docker commands. This requires sudo permission to run Docker. Authentication or daemon-access failures stop startup before either application launches. Cargo and npm always run as your current normal user. For MongoDB diagnostics, use `sudo docker compose logs mongo` when Docker requires sudo.

The launcher starts only the existing Compose `mongo` service, reuses its container without recreating it, and waits up to 180 seconds for its configured health check. It then runs `cargo run -p fuel-ops` and `npm --prefix web run dev`, keeping both services' logs visible in the terminal. Cargo may compile the backend on startup. Open http://localhost:5173; the API defaults to http://127.0.0.1:8080 and MongoDB to mongodb://localhost:27017. The backend reads the existing `.env`; `BIND_ADDRESS` can change its address. Vite prints its actual URL if the default port is occupied; keep the frontend URL consistent with `WEB_ORIGIN`.

Press Ctrl+C to stop the backend, frontend, and their child processes. If either application exits, the launcher stops the other and exits with a failure status. Processes receive a termination signal, followed by forced termination if they have not stopped within five seconds. MongoDB stays running for reuse, and its named volume and existing data are preserved. The launcher never invokes seeding, migration commands, database deletion, or reset commands. Initial setup and any deliberate database maintenance remain manual.

## Demo accounts

Organization: `fuelops-demo`. Every seeded account uses your `DEMO_PASSWORD`.

| Account                    | Access                      |
| -------------------------- | --------------------------- |
| `owner@fuelops.demo`       | Owner of both demo stations |
| `supervisor0@fuelops.demo` | Tehran supervisor           |
| `operator0@fuelops.demo`   | Tehran operator             |
| `viewer0@fuelops.demo`     | Tehran viewer               |
| `supervisor1@fuelops.demo` | Karaj supervisor            |
| `operator1@fuelops.demo`   | Karaj operator              |
| `viewer1@fuelops.demo`     | Karaj viewer                |

Two stations, equipment, employees, configurable roles, 30 days of completed shifts, historical deliveries, inventory movements, active shifts, future schedules, and operational issues are seeded in MongoDB. The dashboard has no hardcoded business datasets.

## Demonstration walkthrough

1. Sign in as the owner and switch between stations in the top bar.
2. Explore dashboard ranges, Persian calendar filters, chart series controls, comparison, and CSV export.
3. In Employees, create an account and assign station-specific roles. Roles can be edited on the permissions screen. Permission changes take effect on the next API request. An administrator cannot edit a role assigned to their own account; use a second authorized administrator.
4. Open the active shift and choose end-of-shift submission. Enter the final reading and allocate the dispensed volume between its historical price tiers. The active demo nozzle begins at 500,000 liters. A final reading of 500,010 liters with 10 liters at the 15,000 IRR tier expects a settlement of 150,000 IRR.
5. Create a reconciliation for that submitted shift. Progress through calculation, discrepancy review, approval, and completion. Explain discrepancies before approval. Completion updates inventory and approves the shift in one transaction.
6. Return to Shifts and close the approved shift with a handover note.
7. Register a delivery, progress to measurement, record direct before/after volumes, review any difference, approve, and complete. Completion adds the approved delivered quantity to recorded inventory atomically.
8. Acknowledge an operational issue, assign it, and resolve it with a note. Audit shows changes and version numbers.

An inventory balance must exist before a delivery or dispensing adjustment can be finalized. Record an initial manual or simulated measurement for a new tank. Measurement timestamps cannot precede the current balance. No tank-height-to-volume conversion is performed without calibration data.

## Manual validation

Compilation is intentionally left to the project owner. Run:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix web run typecheck
npm --prefix web run lint
npm --prefix web test
npm --prefix web run build
```

`cargo test --workspace` includes isolated unit tests. The HTTP/MongoDB integration test is explicitly ignored by default to avoid touching an ordinary development database.

For integration testing, start a separate API process on a different port with `MONGODB_DATABASE=fuel_ops_test`, the same `JWT_SECRET`, `WEB_ORIGIN`, and `MONGODB_URI`, and `BIND_ADDRESS=127.0.0.1:8081`. Do not seed this database. Then run:

```sh
MONGODB_DATABASE=fuel_ops_test TEST_API_BASE=http://127.0.0.1:8081/api/v1 cargo test -p fuel-ops --test integration -- --ignored
```

Export the configuration variables in that terminal before running the test; integration tests do not implicitly load `.env`. The test creates a uniquely scoped organization on each run and refuses databases without the `_test` suffix. It covers protected mutations, station isolation, overlapping assignments, concurrent shift updates, reconciliation arithmetic, atomic inventory changes, late corrections, delivery completion, dashboard totals, password hash redaction, session revocation, and refresh-token replay. Test fixture records are intentionally retained for diagnosis in the disposable test database.

After compilation passes, verify the UI at desktop and mobile widths, keyboard dialog navigation, dark mode, reduced motion, error/empty/loading states, and the demonstration walkthrough. In particular, confirm that a failed API request releases form and navigation locks and that two overlays cannot release each other's locks.

## API and architecture

- `docs/openapi.yaml`: OpenAPI 3.1 contract for versioned REST endpoints.
- `docs/architecture.md`: module APIs, ownership, event durability, concurrency, and reporting semantics.
- `/health`: database-backed readiness response.
- All protected routes start with `/api/v1` and require a short-lived bearer token.
- `X-Station-Id` selects a station already assigned to the authenticated user.
- Login, refresh, and logout require an `Origin` matching `WEB_ORIGIN`.
- Amounts are integer IRR. Volumes are integer milliliters; the UI displays liters. UTC timestamps are integer epoch milliseconds.

## Deployment requirements and current limits

Local Compose is a development configuration, not a secured production database. Use authenticated MongoDB, TLS, private networking, backups, recovery testing, HTTPS, `COOKIE_SECURE=true`, a strong independent JWT secret, and a reverse proxy serving the frontend and `/api` under the same origin. Serve the frontend with SPA fallback to `index.html` and appropriate security headers. Login rate limits use the actual peer address; define and validate trusted proxy handling before placing multiple client addresses behind a proxy.

`Cargo.lock` and `web/package-lock.json` are committed for reproducible dependency resolution. Keep both lockfiles updated when changing dependencies. For a fresh frontend checkout, `npm --prefix web ci` installs the locked versions.

The MVP serializes write commands per organization to protect cross-document invariants. This deliberately favors correctness over peak write throughput; large installations should replace this with narrower validated reservation locks. The API does not automatically replay failed business commands. On conflict or an ambiguous database result, refresh authoritative state before retrying.

Completed meter readings and settlements are immutable. Financial and recorded-inventory corrections are appended separately with authorization and a reason. Volume corrections additionally require inventory management permission and a tank belonging to the original shift. Equipment replacement requires a dedicated reviewed workflow; historical meter records are never silently rewritten. The initial UI focuses on modest station/team sizes; selection lists currently use paginated API results and should gain searchable selectors before very large directories are introduced.

The alert worker persists checkpoints and retries pending events on a timer. Discrepancies, low recorded inventory, and overdue shifts create automatic operational issues; other issue categories are entered manually. This is not certified industrial safety monitoring. IoT and ATG integrations, organization onboarding, password recovery, attachment uploads, and automated bank reconciliation are outside the current UI.

## License

FuelOps is covered by the FuelOps Source-Available License 1.0 in [LICENSE](LICENSE). It permits private, non-commercial education and evaluation. Redistribution and commercial use require prior written permission from the copyright holder. Third-party dependencies remain under their own licenses.
