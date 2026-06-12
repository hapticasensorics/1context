Yep. V2 should be a buildable spec, not another tasteful architecture cloud. This is the plan I would hand to agents.

I’m assuming the screenshot’s repo shape means roughly:

```text
macos/                         Swift menu bar app + daemon/service layer
crates/onecontext-memory-db     Rust memoryd / Perception DB layer
crates/onecontext-context-engine
crates/onecontext-wiki-core
crates/onecontext-wiki-daemon
release/
runtime/
scripts/
```

Exact file names still need repo grep, but the interfaces and ownership below should be locked.

# V2 plan: app-managed private Postgres/Timescale

## 0. Release decision

The release storage architecture is:

```text
1Context menu bar app
  ↓
Swift daemon / onecontextd
  ↓
Rust onecontext-memoryd
  ↓
LocalPostgresSupervisor
  ↓
bundled private PostgreSQL + TimescaleDB
  ↓
~/Library/Application Support/1Context/Postgres/pgdata
```

No Docker. No Colima. No Homebrew. No user-visible port. No user-visible database lifecycle.

`memoryd` owns database truth. Swift owns user experience and service lifecycle. Context Engine and Wiki should treat memory as a required dependency in release mode, not as an optional nice-to-have.

Apple’s own file-system guidance says app-created support files that should remain hidden from the user belong under `Library/Application Support`, while caches belong under `Library/Caches`; so the private database should live in Application Support, not `~/1Context`. ([Apple Developer][1])

---

# 1. Non-negotiable decisions for agents

Put this at the top of the implementation prompt:

```text
Do not replace Postgres/Timescale.
Do not introduce SQLite/DuckDB as canonical storage.
Do not require Docker, Colima, Homebrew, or a user terminal for release.
Do not make Swift run SQL.
Do not make Context Engine silently continue with fake/empty density in release.
Do not expose a release TCP listener.
Do not delete pgdata during repair. Quarantine first.
Do not combine storage bootstrap with recent-history ingestion.
Do not invent a new IPC transport unless the existing one cannot support the methods.
```

The main distinction:

```text
ensure_storage_ready
  means Postgres exists, starts, has Timescale enabled, and schema migrations are applied.

ensure_recent_backfill
  means capture/indexing has populated enough recent memory, especially the last 72 hours.
```

These are separate because storage lifecycle and ingestion semantics are different beasts. Same cave, different teeth.

---

# 2. Runtime modes and flags

Add three explicit storage backend modes:

```text
managed_postgres       release default
external_postgres      gated debug only, requires ONECONTEXT_ALLOW_EXTERNAL_POSTGRES=1
disabled               tests only, only where memory is not required
```

Environment flags:

```text
ONECONTEXT_STORAGE_BACKEND=managed_postgres
ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK=0
ONECONTEXT_BOOTSTRAP_RECENT_DAYS=3
ONECONTEXT_APP_SUPPORT_DIR=<override for tests only>
ONECONTEXT_MANAGED_PG_PREFIX=<override for packaging tests only>
```

Release defaults:

```text
ONECONTEXT_STORAGE_BACKEND=managed_postgres
ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK=0
```

Debug-only external mode:

```text
ONECONTEXT_ALLOW_EXTERNAL_POSTGRES=1
ONECONTEXT_STORAGE_BACKEND=external_postgres
ONECONTEXT_MEMORY_DB_URL=<explicit external database URL>
```

Developers should dogfood `managed_postgres`. Otherwise the product path becomes
a decorative bridge to nowhere.

---

# 3. Directory layout

## Unsandboxed app

```text
~/Library/Application Support/1Context/
  Postgres/
    pgdata/
    run/
    logs/
    auth/
    backups/
    bootstrap/
    repair-quarantine/
  Runtime/
    memoryd.sock
    onecontextd.sock
  Memory/
    backfill-state.json
  Config/
    storage.json

~/Library/Caches/1Context/
  wiki-render-cache/
  derived-density-cache/
  temp-backfill/

~/Library/Logs/1Context/
  onecontextd.log
  memoryd.log
  postgres-supervisor.log
```

## Sandboxed app

If `macos/entitlements.plist` has app sandbox enabled and helpers need shared access, use an App Group container:

```text
~/Library/Group Containers/group.com.onecontext.1context/
  Application Support/1Context/...
```

Agent rule:

```text
If sandbox is enabled, do not write private Postgres data into a per-app container
that the helper cannot access. Use the existing app group if present. If no app
group exists, stop and report the entitlement mismatch.
```

## Permissions

```text
~/Library/Application Support/1Context/              0700
Postgres/                                           0700
Postgres/pgdata/                                    0700
Postgres/run/                                       0700
Postgres/auth/                                      0700
Postgres/auth/*.json                                0600
Postgres/auth/*.pgpass                              0600
Postgres/bootstrap/                                 0700
Postgres/repair-quarantine/                         0700
```

---

# 4. App bundle layout

For the packaging spike, stage this shape:

```text
1Context.app/
  Contents/
    MacOS/
      1Context
      onecontextd
      onecontext-memoryd
      onecontext-context-engine
      onecontext-wiki-daemon

    Resources/
      managed-postgres/
        macos-arm64/
          manifest.json
          bin/
            postgres
            initdb
            pg_ctl
            pg_isready
            psql
            createdb
          lib/
          lib/postgresql/
            timescaledb*.dylib
          share/
          share/postgresql/
          share/postgresql/extension/
            timescaledb.control
            timescaledb--*.sql
```

Apple documents bundle structures as containing executable code and resources, and also has specific guidance for embedding nonstandard code structures; because Postgres plus Timescale is a pile of command-line tools and dylibs, packaging must explicitly sign and verify all Mach-O artifacts rather than assuming the outer app signature magically blesses the whole goblin caravan. ([Apple Developer][2])

Add:

```text
release/managed-postgres/runtime/macos-arm64/manifest.json
```

Manifest:

```json
{
  "bundle_schema": 1,
  "arch": "arm64",
  "postgres_major": 16,
  "postgres_version": "16.x",
  "timescale_version": "x.y.z",
  "build_id": "managed-pg16-ts-x.y.z-arm64",
  "bin": {
    "postgres": "bin/postgres",
    "initdb": "bin/initdb",
    "pg_ctl": "bin/pg_ctl",
    "pg_isready": "bin/pg_isready",
    "psql": "bin/psql"
  },
  "extension": {
    "timescaledb_control": "share/postgresql/extension/timescaledb.control",
    "timescaledb_library_glob": "lib/postgresql/timescaledb*.dylib"
  }
}
```

For v0, I would ship **arm64 first** unless universal packaging already works. x86_64 support should be a separate packaging matrix, not a hidden tax inside the first implementation.

---

# 5. Rust API contract

Add the API first, before the appliance exists.

## New RPC methods

Use existing memory IPC transport. Names:

```text
memory.storage_health
memory.ensure_storage_ready
memory.ensure_recent_backfill
memory.repair_storage
memory.storage_diagnostics
```

Do not add a second control plane.

## PR 1 behavior

Initially:

```text
external_postgres + reachable    → external_postgres_ready
external_postgres + unreachable  → external_postgres_unavailable
managed_postgres                 → managed_postgres_not_implemented
```

That lets Swift and Context Engine wire the contract before packaging Timescale. Tiny PR, big hinge.

## Rust types

Add something like this near the existing memory API types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    ExternalPostgres,
    ManagedPostgres,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageStatus {
    ExternalPostgresReady,
    ExternalPostgresUnavailable,

    ManagedPostgresNotImplemented,
    BundleMissing,
    BundleInvalid,

    DataDirMissing,
    ClusterUninitialized,
    ConfigMissing,

    Stopped,
    Starting,
    RunningButUnhealthy,

    TimescaleMissing,
    TimescaleNotPreloaded,
    TimescaleVersionMismatch,

    SchemaMissing,
    SchemaOutdated,
    Migrating,

    Ready,

    Repairing,
    PermissionDenied,
    DiskFull,
    VersionTooOld,
    VersionTooNew,
    Corrupt,
    Fatal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealth {
    pub backend: StorageBackend,
    pub status: StorageStatus,

    pub ready: bool,
    pub user_action_required: bool,
    pub safe_to_retry: bool,

    pub app_support_dir: Option<String>,
    pub pgdata_dir: Option<String>,
    pub socket_dir: Option<String>,

    pub postgres_major: Option<u16>,
    pub postgres_version: Option<String>,
    pub timescale_version: Option<String>,
    pub expected_timescale_version: Option<String>,

    pub schema_version: Option<i64>,
    pub expected_schema_version: Option<i64>,

    pub recent_backfill: RecentBackfillHealth,

    pub message: String,
    pub detail: Option<String>,
}
```

Backfill stays separate:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackfillStatus {
    NotStarted,
    Running,
    RecentReady,
    Complete,
    Paused,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentBackfillHealth {
    pub status: BackfillStatus,
    pub window_hours: u32,
    pub last_72h_event_count: Option<i64>,
    pub last_successful_ingest_ts: Option<i64>,
    pub message: Option<String>,
}
```

## Example health response

```json
{
  "backend": "managed_postgres",
  "status": "ready",
  "ready": true,
  "user_action_required": false,
  "safe_to_retry": true,
  "app_support_dir": "/Users/paul/Library/Application Support/1Context",
  "pgdata_dir": "/Users/paul/Library/Application Support/1Context/Postgres/pgdata",
  "socket_dir": "/Users/paul/Library/Application Support/1Context/Postgres/run",
  "postgres_major": 16,
  "postgres_version": "16.x",
  "timescale_version": "x.y.z",
  "expected_timescale_version": "x.y.z",
  "schema_version": 12,
  "expected_schema_version": 12,
  "recent_backfill": {
    "status": "recent_ready",
    "window_hours": 72,
    "last_72h_event_count": 18423,
    "last_successful_ingest_ts": 1780620000000,
    "message": "Recent memory is ready."
  },
  "message": "Local memory storage is ready.",
  "detail": null
}
```

---

# 6. Rust module implementation

Add a managed Postgres module under the memory crate.

Likely path:

```text
crates/onecontext-memory-db/src/local_postgres/
  mod.rs
  paths.rs
  manifest.rs
  auth.rs
  config.rs
  process.rs
  supervisor.rs
  bootstrap.rs
  health.rs
  migrations.rs
  repair.rs
  diagnostics.rs
  error.rs
```

If `onecontext-memoryd` is not in `onecontext-memory-db`, agents should put this module beside the actual memoryd storage bootstrap code, but the module boundary stays the same.

## `paths.rs`

Responsibilities:

```text
resolve app support dir
resolve pgdata dir
resolve socket dir
resolve logs dir
resolve auth dir
resolve bootstrap lock path
resolve bundled Postgres prefix
support test override via ONECONTEXT_APP_SUPPORT_DIR
support bundle override via ONECONTEXT_MANAGED_PG_PREFIX
create directories with correct permissions
```

Pseudo-code:

```rust
pub struct ManagedPgPaths {
    pub app_support: PathBuf,
    pub postgres_root: PathBuf,
    pub pgdata: PathBuf,
    pub socket_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub auth_dir: PathBuf,
    pub bootstrap_dir: PathBuf,
    pub repair_quarantine_dir: PathBuf,
    pub bootstrap_lock: PathBuf,
}

impl ManagedPgPaths {
    pub fn resolve() -> Result<Self>;
    pub fn create_all_secure(&self) -> Result<()>;
}
```

## `manifest.rs`

Responsibilities:

```text
load manifest.json
verify current CPU architecture
verify required binaries exist and are executable
verify timescaledb control file exists
verify timescaledb dylib exists
verify expected Postgres major
return BundleMissing or BundleInvalid without mutation
```

Pseudo-code:

```rust
pub struct ManagedPgManifest {
    pub postgres_major: u16,
    pub postgres_version: String,
    pub timescale_version: String,
    pub arch: String,
    pub prefix: PathBuf,
    pub postgres_bin: PathBuf,
    pub initdb_bin: PathBuf,
    pub pg_ctl_bin: PathBuf,
    pub pg_isready_bin: PathBuf,
    pub psql_bin: PathBuf,
}
```

## `auth.rs`

Use generated passwords, but do not use `PGPASSWORD` in release. PostgreSQL documents `PGPASSWORD`, but warns it is not recommended because process environments may be visible; use a password file or direct in-process client config instead. ([PostgreSQL][3])

V0 implementation:

```text
Postgres/auth/auth.json      0600
Postgres/auth/postgres.pgpass 0600
Postgres/auth/app.pgpass      0600
```

`auth.json`:

```json
{
  "schema": 1,
  "postgres_user": "postgres",
  "postgres_password": "<random-32-byte-base64>",
  "app_user": "onecontext",
  "app_password": "<random-32-byte-base64>",
  "created_at": "2026-06-05T00:00:00Z"
}
```

Agent rule:

```text
V0 may store these secrets in 0600 files under Application Support.
Do not block v0 on Keychain integration.
Add a TODO for Keychain hardening later.
```

Rationale: without DB encryption, a same-user local adversary can already target files in the user account. This password prevents casual accidental access, not a full local-compromise threat model.

## `config.rs`

Write managed `postgresql.conf`:

```conf
listen_addresses = ''
unix_socket_directories = '/Users/<user>/Library/Application Support/1Context/Postgres/run'
unix_socket_permissions = 0700
port = 15432

shared_preload_libraries = 'timescaledb'

max_connections = 32
shared_buffers = 128MB
work_mem = 8MB
maintenance_work_mem = 64MB

logging_collector = on
log_directory = '/Users/<user>/Library/Application Support/1Context/Postgres/logs'
log_filename = 'postgresql-%Y-%m-%d.log'
log_rotation_age = 1d
log_rotation_size = 10MB

log_min_messages = warning
log_min_error_statement = error
```

PostgreSQL documents that `listen_addresses = ''` means the server listens on no TCP/IP interface, leaving only Unix-domain socket access; it also documents `unix_socket_directories` and `unix_socket_permissions`, including `0700` as a reasonable permission option. ([PostgreSQL][4])

Write `pg_hba.conf`:

```conf
# TYPE  DATABASE     USER        ADDRESS       METHOD
local   all          postgres                  scram-sha-256
local   onecontext   onecontext                scram-sha-256
local   all          all                       reject

host    all          all         127.0.0.1/32  reject
host    all          all         ::1/128       reject
```

PostgreSQL’s `pg_hba.conf` is the client authentication control file for the cluster and is created when `initdb` initializes the data directory. ([PostgreSQL][5])

## `process.rs`

Use `pg_ctl` for start/stop/status and `pg_isready` for readiness. PostgreSQL describes `pg_ctl` as the utility for initializing, starting, stopping, restarting, and checking server status, and notes that it wraps useful behavior like log redirection and controlled shutdown. ([PostgreSQL][6])

Commands memoryd should run:

```bash
pg_ctl \
  -D "$PGDATA" \
  -l "$LOG_DIR/postgres-supervisor.log" \
  -o "-c config_file=$PGDATA/postgresql.conf" \
  start
```

Readiness:

```bash
pg_isready \
  -h "$SOCKET_DIR" \
  -p 15432 \
  -d onecontext \
  -U onecontext
```

Shutdown:

```bash
pg_ctl -D "$PGDATA" stop -m fast -t 15
```

Status:

```bash
pg_ctl -D "$PGDATA" status
```

Do not start Postgres directly from Swift. Swift calls memoryd. memoryd invokes `pg_ctl`.

## `bootstrap.rs`

Bootstrap order:

```text
1. Acquire bootstrap.lock.
2. Resolve paths.
3. Load bundle manifest.
4. Create secure directories.
5. Generate auth files if missing.
6. If pgdata missing, run initdb.
7. Write postgresql.conf.
8. Write pg_hba.conf.
9. Start Postgres.
10. Wait for pg_isready.
11. Create app role and database.
12. Enable TimescaleDB.
13. Run migrations.
14. Verify health.
15. Release lock.
```

Use `initdb` like:

```bash
initdb \
  -D "$PGDATA" \
  --username=postgres \
  --auth-local=scram-sha-256 \
  --auth-host=scram-sha-256 \
  --pwfile "$BOOTSTRAP_DIR/postgres-superuser.pw" \
  --encoding=UTF8 \
  --locale=C
```

`initdb` creates the PostgreSQL database cluster, accepts `--pgdata`, supports `--auth-local`, and can read the bootstrap superuser password from `--pwfile`. ([PostgreSQL][7])

After first start, run:

```sql
CREATE ROLE onecontext LOGIN PASSWORD '<app_password>';
CREATE DATABASE onecontext OWNER onecontext;
```

Then on the `onecontext` database:

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;
```

Timescale’s self-hosted install docs specify adding TimescaleDB to `shared_preload_libraries` and then running `CREATE EXTENSION IF NOT EXISTS timescaledb;`. ([TigerData][8])

## `migrations.rs`

Do not invent a migration system if one exists. Wrap the existing migration runner.

Required behavior:

```text
create schema_migrations table if missing
acquire pg_advisory_lock for migrations
run migrations in order
one transaction per migration unless marked no_transaction
record checksum
fail closed on checksum mismatch
never continue with partially unknown schema
```

Advisory lock name:

```sql
SELECT pg_advisory_lock(hashtext('onecontext:memory:migrations'));
```

Health checks after migration:

```sql
SELECT extversion
FROM pg_extension
WHERE extname = 'timescaledb';

SELECT version
FROM perception.schema_migrations
ORDER BY version DESC
LIMIT 1;
```

Timescale checks:

```sql
SELECT hypertable_schema, hypertable_name
FROM timescaledb_information.hypertables;
```

If existing schema names differ, agents should use existing names, not rename the world.

---

# 7. Storage state machine

`storage_health` must be read-only. It detects.

`ensure_storage_ready` may mutate. It repairs.

## Detection order

```text
BundleMissing
  if manifest or required binaries missing

BundleInvalid
  if binary missing, not executable, wrong arch, or Timescale files missing

DataDirMissing
  if pgdata directory does not exist

ClusterUninitialized
  if pgdata exists but PG_VERSION missing

ConfigMissing
  if postgresql.conf or pg_hba.conf missing

Stopped
  if cluster exists but pg_isready fails and no live postmaster

RunningButUnhealthy
  if pg_ctl status says running but SQL connect fails

TimescaleMissing
  if extension cannot be created or files unavailable

TimescaleNotPreloaded
  if CREATE EXTENSION fails because shared_preload_libraries is wrong

SchemaMissing
  if onecontext/perception schema missing

SchemaOutdated
  if migration version < expected

Ready
  if all checks pass
```

## Repair actions

```text
DataDirMissing       → initdb
ClusterUninitialized → initdb after moving bad partial dir to quarantine
ConfigMissing        → rewrite config
Stopped              → start
RunningButUnhealthy  → stop fast, start, recheck
TimescaleMissing     → fatal unless bundle is invalid or config fixable
TimescaleNotPreloaded→ rewrite config, restart, create extension
SchemaMissing        → run migrations
SchemaOutdated       → run migrations
Corrupt              → quarantine pgdata, create fresh cluster, schedule salvage
DiskFull             → stop, surface user action
VersionTooNew        → refuse downgrade, preserve data
```

Never:

```text
rm -rf pgdata
```

Always:

```text
mv pgdata repair-quarantine/pgdata-<timestamp>-<reason>
```

---

# 8. Recent backfill contract

Add:

```text
memory.ensure_recent_backfill
```

Request:

```json
{
  "window_hours": 72,
  "reason": "wiki_refresh",
  "min_event_count": 100,
  "block_until_ready": false
}
```

Response:

```json
{
  "status": "running",
  "window_hours": 72,
  "event_count": 421,
  "sessions_count": 18,
  "last_successful_ingest_ts": 1780620000000,
  "message": "Building recent memory."
}
```

This method should not create or start Postgres directly. It begins with:

```text
ensure_storage_ready
```

Then it delegates to existing capture/index/backfill infrastructure.

Minimum useful threshold for wiki:

```text
recent memory is useful if:
  event_count >= configured threshold
  OR sessions_count >= configured threshold
  OR at least one source has nonzero density in last 72h
```

The threshold should be configurable:

```text
ONECONTEXT_MIN_RECENT_EVENTS_FOR_WIKI=100
ONECONTEXT_MIN_RECENT_SESSIONS_FOR_WIKI=5
```

---

# 9. Context Engine release gate

Modify `crates/onecontext-context-engine`.

Current behavior to kill in release:

```text
memory unavailable → empty density → tiny wiki
```

New behavior:

```text
if ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK=0:
    call memory.ensure_storage_ready(reason = "context_engine_preflight")
    if not ready:
        return typed StorageNotReady error

    call memory.ensure_recent_backfill(window_hours = 72)
    if recent backfill not useful:
        return typed RecentMemoryBuilding status, not fake empty density

    query density
else:
    existing dev fallback allowed
```

Error enum:

```rust
pub enum ContextPlanningError {
    StorageNotReady(StorageHealth),
    RecentMemoryBuilding(RecentBackfillHealth),
    MemoryUnavailableButFallbackAllowed,
    DensityQueryFailed(anyhow::Error),
}
```

For release UX, `RecentMemoryBuilding` is not failure. It maps to:

```text
Building local memory from recent activity…
```

`StorageNotReady` maps to:

```text
Starting local memory…
Repairing local memory…
Needs attention…
```

---

# 10. Swift daemon and menu implementation

## Ownership

Swift owns:

```text
menu bar item
daemon registration
daemon health polling
user-visible state
repair/reset/diagnostics actions
launching/restarting memoryd if current architecture does that
```

Swift does not own:

```text
initdb
pg_ctl
SQL
CREATE EXTENSION
migrations
schema checks
hypertable checks
```

Apple documents `SMAppService` for registering and controlling LoginItems, LaunchAgents, and LaunchDaemons on macOS 13 and later, so the Swift side should use the existing ServiceManagement path if present, or `SMAppService` if adding a modern launch helper. ([Apple Developer][9])

## Process tree

Preferred:

```text
1Context.app menu bar
  ↓ IPC
onecontextd Swift LaunchAgent
  ↓ starts/supervises
onecontext-memoryd Rust process
  ↓ pg_ctl/process supervision
private postgres
```

Do not add a separate LaunchAgent for Postgres.

## Swift state enum

Add something like:

```swift
enum LocalMemoryState: Equatable {
    case unknown
    case externalReady
    case externalUnavailable

    case starting
    case initializingStorage
    case migratingSchema
    case buildingRecentHistory(progress: RecentHistoryProgress?)
    case ready

    case repairing(message: String)
    case needsAttention(issue: LocalMemoryIssue)
    case paused
}
```

Issue enum:

```swift
enum LocalMemoryIssue: Equatable {
    case diskFull
    case permissionDenied
    case bundleMissing
    case bundleInvalid
    case timescaleMissing
    case schemaMigrationFailed
    case corruptStorage
    case versionTooNew
    case fatal(message: String)
}
```

## Menu labels

Top-level menu:

```text
1Context
────────────────────────
Local Memory: Ready
Wiki: Last refreshed 2m ago
────────────────────────
Refresh Wiki
Build Recent Memory
Pause Capture
Open Wiki
────────────────────────
Repair Local Memory…
Diagnostics…
Reset Local Memory…
Quit
```

State text mapping:

```text
external_postgres_ready           → Local Memory: Ready
external_postgres_unavailable     → Local Memory: Dev DB unavailable
managed_postgres_not_implemented  → Local Memory: Managed storage not available

bundle_missing                    → Local Memory: Needs attention
data_dir_missing                  → Local Memory: Starting…
cluster_uninitialized             → Local Memory: Initializing…
stopped                           → Local Memory: Starting…
timescale_missing                 → Local Memory: Needs attention
schema_outdated                   → Local Memory: Updating…
ready + recent not ready          → Local Memory: Building recent history…
ready + recent ready              → Local Memory: Ready
disk_full                         → Local Memory: Needs disk space
permission_denied                 → Local Memory: Permission issue
corrupt                           → Local Memory: Repairing…
fatal                             → Local Memory: Needs attention
```

## Refresh Wiki flow

```swift
func refreshWiki() async {
    setMenuState(.starting)

    let storage = await memory.ensureStorageReady(reason: "wiki_refresh")
    guard storage.ready else {
        setMenuState(mapStorageHealth(storage))
        return
    }

    setMenuState(.buildingRecentHistory(progress: nil))

    let backfill = await memory.ensureRecentBackfill(windowHours: 72, reason: "wiki_refresh")
    if !backfill.isUseful {
        setMenuState(.buildingRecentHistory(progress: backfill.progress))
        // Keep polling. Do not show successful tiny wiki.
        return
    }

    setMenuState(.refreshingWiki)

    let result = await wiki.refresh(strictMemory: true)
    setMenuState(result.success ? .ready : .needsAttention(...))
}
```

For v0, Swift can poll `memory.storage_health` every 1 second while `ensure_storage_ready` or `ensure_recent_backfill` runs. No streaming requirement yet.

---

# 11. Packaging spike, highest-risk track

This starts immediately in parallel with PR 1.

## Goal

Produce a notarized toy macOS app bundle that contains:

```text
postgres
initdb
pg_ctl
pg_isready
psql
timescaledb dylibs
timescaledb extension SQL/control files
```

And can run:

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;
SELECT extversion FROM pg_extension WHERE extname='timescaledb';
```

from inside a clean macOS user account with no Homebrew, Docker, Colima, or shell setup.

## Scripts to add

```text
release/macos/stage-managed-postgres.sh
release/macos/build-timescale-for-bundled-postgres.sh
release/macos/sign-managed-postgres.sh
release/macos/verify-managed-postgres.sh
release/macos/smoke-managed-postgres-app.sh
```

## Packaging script behavior

`stage-managed-postgres.sh`:

```text
1. Create release/managed-postgres/runtime/macos-arm64.
2. Stage pinned PostgreSQL prefix.
3. Build or stage TimescaleDB against that exact pg_config.
4. Copy Timescale extension control/sql files into share/postgresql/extension.
5. Copy Timescale dylibs into lib/postgresql.
6. Rewrite absolute install names if needed.
7. Write manifest.json.
8. Run otool -L checks.
9. Run codesign checks.
```

`verify-managed-postgres.sh`:

```bash
set -euo pipefail

APP="$1"

codesign --verify --strict --deep --verbose=4 "$APP"
spctl --assess --type execute --verbose "$APP" || true

find "$APP/Contents/Resources/managed-postgres" -type f -perm +111 -print
find "$APP/Contents/Resources/managed-postgres" -name "*.dylib" -print

# fail if otool shows Homebrew/MacPorts absolute dylibs
otool -L "$APP/Contents/Resources/managed-postgres/macos-arm64/bin/postgres"
```

Acceptance criteria:

```text
1. App signs.
2. App notarizes.
3. App launches on clean machine.
4. Bundled postgres starts from app resources.
5. CREATE EXTENSION timescaledb succeeds.
6. otool does not show accidental /opt/homebrew dependency paths.
7. No TCP listener is created.
8. Unix socket connection works.
```

## License gate

Timescale’s legal page says TimescaleDB Open Source is Apache 2.0, while TimescaleDB Community is under the Timescale License, and says Community features are free as long as you are not offering TimescaleDB as hosted DBaaS. That still needs counsel or founder signoff before distribution, especially if you bundle Community bits. ([TigerData][10])

Agent rule:

```text
Do not silently choose Apache-only or Community build.
Report which Timescale artifacts are staged and which license applies.
```

---

# 12. Build and release integration

Add release manifest validation to CI:

```text
cargo test -p onecontext-memory-db local_postgres_manifest
swift test --package-path macos
release/macos/verify-managed-postgres.sh dist/1Context.app
```

Add a smoke test target:

```text
scripts/smoke-managed-storage.sh
```

Behavior:

```bash
#!/usr/bin/env bash
set -euo pipefail

export ONECONTEXT_STORAGE_BACKEND=managed_postgres
export ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK=0
export ONECONTEXT_APP_SUPPORT_DIR="$(mktemp -d)"

./target/release/onecontext-memoryd storage-health
./target/release/onecontext-memoryd ensure-storage-ready
./target/release/onecontext-memoryd storage-health

# Insert/query tiny perception fixture.
./target/release/onecontext-memoryd dev-insert-density-fixture
./target/release/onecontext-context-engine plan --window-hours 72

# Kill postgres and verify repair.
pkill -f "Application Support/1Context/Postgres/pgdata" || true
./target/release/onecontext-memoryd ensure-storage-ready
./target/release/onecontext-memoryd storage-health
```

Acceptance:

```text
No Docker process exists.
No Colima process exists.
No tcp listener on 15432.
Postgres responds through Unix socket.
Context Engine refuses fake empty memory in release mode.
```

---

# 13. PR sequence

## Track A, packaging spike, starts immediately

**Goal:** determine whether Timescale bundling is clean enough.

Files likely touched:

```text
release/
runtime/
scripts/
macos/tools/
```

Deliverables:

```text
release/managed-postgres/runtime/macos-arm64/manifest.json
release/macos/stage-managed-postgres.sh
release/macos/verify-managed-postgres.sh
toy smoke test proving CREATE EXTENSION works
```

Exit criteria:

```text
Bundled Postgres + Timescale runs after signing/notarization.
No Homebrew dynamic library leakage.
No Docker/Colima involved.
```

If this fails, do not let agents invent a database fallback. Escalate.

---

## PR 1, storage API skeleton

**Goal:** add contract with current external DB behavior only.

Files likely touched:

```text
crates/onecontext-memory-db/...
crates/*memoryd*/...
```

Implementation:

```text
memory.storage_health
memory.ensure_storage_ready
StorageBackend
StorageStatus
StorageHealth
```

Behavior:

```text
external_postgres ready/unavailable
managed_postgres not implemented
```

Acceptance:

```text
memoryd can report storage health.
No packaging required.
No behavior change to wiki yet.
```

---

## PR 2, Context Engine release gate

**Goal:** stop fake tiny wiki in release mode.

Files likely touched:

```text
crates/onecontext-context-engine/...
```

Implementation:

```text
read ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK
preflight memory.ensure_storage_ready
return typed error if storage not ready
allow old fallback only when flag is true
```

Acceptance:

```text
With fallback disabled and DB down, context-engine does not return empty density as success.
```

---

## PR 3, LocalPostgresSupervisor state detection

**Goal:** detect managed storage states without mutating.

Files:

```text
crates/onecontext-memory-db/src/local_postgres/*
```

Implementation:

```text
paths
manifest
health detection
pgdata detection
pg_ctl status
pg_isready check
SQL connect check
```

Acceptance:

```text
managed_postgres + no bundle → bundle_missing
managed_postgres + bundle + no pgdata → data_dir_missing
managed_postgres + stopped cluster → stopped
```

---

## PR 4, bundle Postgres into app artifact

**Goal:** app contains database appliance files.

Files:

```text
release/
runtime/
macos/Package.swift
macos/tools/
```

Implementation:

```text
copy managed-postgres resources into app
sign nested binaries/dylibs
verify manifest
```

Acceptance:

```text
1Context.app contains required DB files.
codesign verification passes.
```

---

## PR 5, init/start private Postgres

**Goal:** clean machine can initialize and start local Postgres.

Implementation:

```text
secure dirs
auth files
bootstrap lock
initdb
postgresql.conf
pg_hba.conf
pg_ctl start
pg_isready
create role/database
```

Acceptance:

```text
ONECONTEXT_STORAGE_BACKEND=managed_postgres memory.ensure_storage_ready works on clean account.
```

---

## PR 6, Timescale + migrations

**Goal:** managed Postgres becomes real Perception DB.

Implementation:

```text
shared_preload_libraries timescaledb
CREATE EXTENSION IF NOT EXISTS timescaledb
run existing schema migrations
verify required hypertables/tables
```

Acceptance:

```text
memoryd can query real Perception DB metadata through managed Postgres.
Manual schema setup no longer required.
```

---

## PR 7, recent backfill separation

**Goal:** last 72 hours becomes an explicit readiness layer.

Implementation:

```text
memory.ensure_recent_backfill
backfill health in storage_health
context-engine waits for useful recent memory
```

Acceptance:

```text
Storage ready but no recent data shows “Building recent history,” not “wiki refreshed.”
```

---

## PR 8, Swift menu integration

**Goal:** user-visible product flow.

Files:

```text
macos/Sources/...
macos/Tests/...
```

Implementation:

```text
poll storage_health
call ensure_storage_ready on launch or first refresh
call ensure_recent_backfill before wiki refresh
menu states
repair action
diagnostics action
```

Acceptance:

```text
Clean install, click Refresh Wiki, app starts local memory and shows honest progress.
```

---

## PR 9, repair and diagnostics

**Goal:** broken states become recoverable.

Implementation:

```text
stale postmaster.pid handling
restart with backoff
quarantine partial/corrupt pgdata
disk full detection
permission errors
diagnostics export
redacted logs
```

Acceptance:

```text
Kill Postgres, refresh recovers.
Stale pid recovers.
Corrupt cluster quarantines or produces clear fatal error without deletion.
```

---

## PR 10, no-Docker release smoke test

**Goal:** prove release promise.

Implementation:

```text
clean app support dir
launch app/daemon
ensure storage
run migrations
run context-engine plan
run wiki dry refresh
verify no TCP listener
verify no Docker/Colima dependency
```

Acceptance:

```text
A clean macOS user gets useful wiki progress with no developer bootstrap.
```

---

# 14. Diagnostics UX

Add “Diagnostics…” menu item.

It should show:

```text
Local Memory
  Backend: Managed Postgres/Timescale
  Status: Ready
  Schema: 12/12
  Recent memory: 18,423 events, 94 sessions in last 72h
  Last repair: never

Storage
  App Support: ~/Library/Application Support/1Context
  Database: present
  Socket: private Unix socket
  TCP listener: none

Actions
  Copy Diagnostics
  Export Redacted Logs
  Repair Local Memory
  Reset Local Memory
```

Do not show raw DB passwords, event payloads, captured text, or object contents.

---

# 15. Reset and uninstall behavior

Menu action:

```text
Reset Local Memory…
```

Flow:

```text
1. Confirm destructive action.
2. Stop capture/backfill.
3. Stop Postgres.
4. Move pgdata to repair-quarantine/reset-<timestamp>.
5. Create fresh storage on next ensure_storage_ready.
6. Offer “Delete old quarantined data” separately.
```

Do not make app uninstall delete memory silently.

Dragging app to Trash should leave:

```text
~/Library/Application Support/1Context/
```

User-controlled deletion should remove it.

---

# 16. Exact agent instructions

Give agents this prompt skeleton:

```text
You are implementing managed local Postgres/Timescale for 1Context.

Architecture is fixed:
- Swift owns menu/daemon UX.
- Rust memoryd owns storage truth.
- Postgres/Timescale is bundled and private.
- Release uses Unix socket only.
- Docker/Colima/Homebrew are not product or release dependencies.
- ensure_storage_ready and ensure_recent_backfill are separate.

Do not redesign storage.
Do not introduce SQLite/DuckDB.
Do not make Swift run SQL.
Do not silently fall back to empty density in release.

For every PR:
1. Identify existing files/symbols first.
2. Patch the smallest surface.
3. Add tests or smoke command.
4. Preserve only gated debug external_postgres mode.
5. Return typed errors, not strings.
6. Never delete pgdata. Quarantine first.
```

---

# 17. What agents should grep first

Before coding, each agent should run these searches and paste results into the PR description:

```bash
rg "Refresh Wiki|refreshWiki|wiki refresh|Wiki" macos crates
rg "memoryd|onecontext-memoryd|memory" macos crates
rg "DATABASE_URL|POSTGRES|PGHOST|PGPORT|timescale|Timescale" .
rg "schema_migrations|migrations|CREATE EXTENSION|hypertable|time_bucket" crates runtime scripts
rg "LaunchAgent|SMAppService|login item|launchctl|daemon" macos
rg "Application Support|Caches|Logs|FileManager" macos crates
rg "empty.*density|fallback|unavailable|metadata" crates/onecontext-context-engine crates
```

This keeps the agents in archaeology mode before they enter patch mode. Less jazz solo, more subway map.

---

# 18. Definition of done

The release architecture is done when this works on a clean non-dev machine:

```text
1. Install 1Context.app.
2. Launch app.
3. Click Refresh Wiki.
4. App starts onecontextd.
5. onecontextd starts memoryd.
6. memoryd initializes private Postgres.
7. memoryd enables Timescale.
8. memoryd runs schema migrations.
9. memoryd builds recent 72h memory.
10. Context Engine sees real density.
11. Wiki refresh produces meaningful output.
12. User never sees Docker, Colima, Homebrew, psql, port, or shell setup.
```

And these failure tests pass:

```text
Kill postgres              → app repairs and continues
Delete config              → app rewrites config
Stale postmaster.pid       → app recovers
Schema outdated            → app migrates
Disk full                  → app stops and tells user plainly
Corrupt pgdata             → app quarantines, never silently deletes
Timescale missing          → app reports packaging/storage fatal clearly
DB unavailable in release  → no tiny fake wiki
```

# My strongest implementation recommendation

Do PR 1 and the packaging spike in parallel.

The API contract is cheap and unlocks Swift plus Context Engine wiring. The packaging spike is the dragon. If Timescale inside a signed/notarized app behaves, this plan is the release path. If it doesn’t, you want to know before agents spend a week polishing a supervisor for a binary bundle that macOS treats like cursed luggage.

[1]: https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html "File System Basics"
[2]: https://developer.apple.com/documentation/bundleresources/placing-content-in-a-bundle?utm_source=chatgpt.com "Placing content in a bundle"
[3]: https://www.postgresql.org/docs/current/libpq-envars.html?utm_source=chatgpt.com "Documentation: 18: 32.15. Environment Variables - PostgreSQL"
[4]: https://www.postgresql.org/docs/current/runtime-config-connection.html "PostgreSQL: Documentation: 18: 19.3. Connections and Authentication"
[5]: https://www.postgresql.org/docs/current/auth-pg-hba-conf.html "PostgreSQL: Documentation: 18: 20.1. The pg_hba.conf File"
[6]: https://www.postgresql.org/docs/current/app-pg-ctl.html "PostgreSQL: Documentation: 18: pg_ctl"
[7]: https://www.postgresql.org/docs/current/app-initdb.html "PostgreSQL: Documentation: 18: initdb"
[8]: https://www.tigerdata.com/docs/get-started/choose-your-path/install-timescaledb?utm_source=chatgpt.com "Install self-hosted TimescaleDB | Tiger Data Docs"
[9]: https://developer.apple.com/documentation/servicemanagement/smappservice?utm_source=chatgpt.com "SMAppService | Apple Developer Documentation"
[10]: https://www.tigerdata.com/legal/licenses "Software Licensing: Timescale License (TSL) | Tiger Data"
