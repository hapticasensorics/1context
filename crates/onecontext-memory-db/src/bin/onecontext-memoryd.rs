use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use onecontext_memory_db::{
    default_context_for_source, default_home_dir, describe_memory_query_protocol, edges, hydrate,
    ingest_claude_incremental, ingest_codex_incremental, ingest_imessage_incremental,
    query_density, read_viewport, search, FileIngestCursor, IncrementalIngestBatch,
    IncrementalIngestOptions, LocalIngestCursors, ManagedPostgresCommandRunner, MethodName,
    ProtocolError, ProtocolRequest, ProtocolResponse, ProtocolStats, SessionIngestProfile,
    SqliteIngestCursor, StatusResponse, StorageBackend, StorageHealth, StorageStatus,
};
use postgres::{Client, NoTls};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

const DEFAULT_INTERVAL_MS: u64 = 60_000;
const DEFAULT_MAX_EVENTS: usize = 1_000;
const DEFAULT_MAX_LINES: usize = 50_000;
const DEFAULT_SOURCES: &str = "codex,claude,imessage";
const DATABASE_URL_ENV: &str = "ONECONTEXT_MEMORY_DB_URL";
const STORAGE_BACKEND_ENV: &str = "ONECONTEXT_STORAGE_BACKEND";
const APP_SUPPORT_DIR_ENV: &str = "ONECONTEXT_APP_SUPPORT_DIR";
const LOCAL_USER_ID: &str = "00000000-0000-0000-0000-000000000001";
const RECENT_BACKFILL_CURSOR_NAME: &str = "memoryd_recent_backfill";
const REQUIRED_EXTENSIONS: &[(&str, bool)] = &[
    ("timescaledb", true),
    ("btree_gist", false),
    ("pgcrypto", false),
    ("pg_trgm", false),
    ("vector", false),
    ("pg_stat_statements", true),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().cloned().unwrap_or_else(|| "help".to_string());
    if command == "help" || command == "--help" || command == "-h" {
        print_usage();
        return Ok(());
    }
    args.remove(0);

    match command.as_str() {
        "daemon" => run_daemon(DaemonOptions::parse(&mut args)?)?,
        "bench" => run_bench(DaemonOptions::parse(&mut args)?)?,
        "status" => run_status(DaemonOptions::parse(&mut args)?)?,
        "describe" => run_describe()?,
        "queryViewport" => run_query_viewport(QueryViewportOptions::parse(&mut args)?)?,
        "queryDensity" => run_query_density(parse_query_density_request(&mut args)?)?,
        "hydrateObjects" => run_hydrate_objects(parse_hydrate_objects_request(&mut args)?)?,
        "protocol" | "rpc" => run_protocol(&mut args)?,
        "query" => run_query(&mut args)?,
        other => {
            eprintln!("unknown command {other:?}");
            print_usage();
            std::process::exit(2);
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct DaemonOptions {
    home: PathBuf,
    context_engine_root: PathBuf,
    run_dir: PathBuf,
    database: Option<DatabaseTarget>,
    interval_ms: u64,
    max_events: usize,
    max_lines: usize,
    sources: Vec<String>,
    session_profile: SessionIngestProfile,
    include_sensitive_text: bool,
    once: bool,
}

#[derive(Debug, Clone)]
struct DatabaseTarget {
    url: String,
    source: String,
}

impl DaemonOptions {
    fn parse(args: &mut Vec<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let home = take_option_value(args, "--home")
            .map(PathBuf::from)
            .unwrap_or_else(default_home_dir);
        let context_engine_root = take_option_value(args, "--context-engine-root")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("1Context/context-engine"));
        let run_dir = take_option_value(args, "--run-dir")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("Library/Application Support/1Context/run"));
        let interval_ms = take_option_value(args, "--interval-ms")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INTERVAL_MS);
        let max_events = take_option_value(args, "--max-events")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_EVENTS);
        let max_lines = take_option_value(args, "--max-lines")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_LINES);
        let sources = take_option_value(args, "--sources")
            .unwrap_or_else(|| DEFAULT_SOURCES.to_string())
            .split(',')
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let session_profile = take_option_value(args, "--profile")
            .as_deref()
            .map(parse_session_profile)
            .transpose()?
            .unwrap_or_default();
        let include_sensitive_text = take_flag(args, "--include-sensitive-text");
        let database = resolve_database_target(args);
        let once = take_flag(args, "--once");

        Ok(Self {
            home,
            context_engine_root,
            run_dir,
            database,
            interval_ms,
            max_events,
            max_lines,
            sources,
            session_profile,
            include_sensitive_text,
            once,
        })
    }

    fn cursor_file(&self) -> PathBuf {
        self.context_engine_root
            .join("memory-db/cursors/local-source-cursors.json")
    }

    fn status_file(&self) -> PathBuf {
        self.run_dir.join("memoryd-status.json")
    }

    fn pid_file(&self) -> PathBuf {
        self.run_dir.join("memoryd.pid")
    }

    fn ingest_options(&self) -> IncrementalIngestOptions {
        IncrementalIngestOptions {
            max_events: self.max_events,
            max_lines: self.max_lines,
            include_sensitive_text: self.include_sensitive_text,
            session_profile: self.session_profile,
        }
    }
}

fn run_daemon(options: DaemonOptions) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&options.run_dir)?;
    if let Some(parent) = options.cursor_file().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        options.pid_file(),
        serde_json::to_string_pretty(&json!({
            "pid": std::process::id(),
            "executable": std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string()),
            "started_at_ms": unix_timestamp_millis(),
            "kind": "onecontext-memoryd"
        }))?,
    )?;
    write_status(&options, "starting", None)?;

    loop {
        let tick_started = Instant::now();
        let tick = run_ingest_tick(&options);
        let elapsed_ms = tick_started.elapsed().as_millis() as u64;
        let status = match tick {
            Ok(mut payload) => {
                payload["daemon_elapsed_ms"] = json!(elapsed_ms);
                payload
            }
            Err(error) => json!({
                "status": "error",
                "schema_version": 1,
                "service": "onecontext-memoryd",
                "error": error.to_string(),
            }),
        };
        write_status(&options, "ok", Some(status))?;

        if options.once {
            break;
        }
        thread::sleep(Duration::from_millis(options.interval_ms));
    }

    Ok(())
}

fn run_bench(options: DaemonOptions) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let mut results = Vec::new();
    for source in &options.sources {
        let mut cursors = LocalIngestCursors::default();
        let cold = timed_ingest_source(&options, source, &mut cursors);
        let hot = timed_ingest_source(&options, source, &mut cursors);
        let mut steady_cursors = eof_cursors_for_source(&options, source);
        let steady = timed_ingest_source(&options, source, &mut steady_cursors);
        results.push(json!({
            "source": source,
            "cold": cold.summary,
            "hot_after_cursor": hot.summary,
            "steady_no_new": steady.summary,
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "ok",
            "schema_version": 1,
            "service": "onecontext-memoryd",
            "command": "bench",
            "created_at": now(),
            "elapsed_ms": started.elapsed().as_millis() as u64,
            "options": options_payload(&options),
            "results": results,
        }))?
    );
    Ok(())
}

fn run_status(options: DaemonOptions) -> Result<(), Box<dyn std::error::Error>> {
    let daemon_status = if options.status_file().exists() {
        match fs::read(options.status_file())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        {
            Some(value) => value,
            None => json!({"status": "unreadable"}),
        }
    } else {
        json!({"status": "not_found"})
    };
    print_json(&json!({
        "status": "ok",
        "schema_version": 1,
        "service": "onecontext-memoryd",
        "command": "status",
        "created_at": now(),
        "status_file": options.status_file(),
        "protocol": describe_memory_query_protocol(),
        "daemon": daemon_status,
        "storage_posture": storage_posture_payload(&options),
    }))
}

fn run_describe() -> Result<(), Box<dyn std::error::Error>> {
    print_json(&describe_memory_query_protocol())
}

fn run_protocol(args: &mut Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let request_json = take_option_value_required(args, "--request-json")?;
    let database = resolve_database_target(args);
    let positional_method = take_positional_protocol_method(args)?;
    let request = if let Some(path) = request_json {
        read_protocol_request(&path, positional_method)?
    } else {
        build_protocol_request_from_args(args, positional_method)?
    };
    ensure_no_args(args)?;
    let response =
        dispatch_protocol_request(request, database, started.elapsed().as_millis() as u64);
    print_json(&response)
}

fn take_positional_protocol_method(
    args: &mut Vec<String>,
) -> Result<Option<MethodName>, Box<dyn std::error::Error>> {
    let Some(index) = args.iter().position(|arg| !arg.starts_with("--")) else {
        return Ok(None);
    };
    Ok(Some(args.remove(index).parse::<MethodName>()?))
}

fn read_protocol_request(
    path: &str,
    positional_method: Option<MethodName>,
) -> Result<ProtocolRequest<Value>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    if path == "-" {
        std::io::stdin().read_to_end(&mut bytes)?;
    } else {
        bytes = fs::read(path)?;
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let mut request = if value.get("method").is_some() {
        serde_json::from_value::<ProtocolRequest<Value>>(value)?
    } else {
        let method = positional_method
            .ok_or("--request-json without an envelope requires a positional method")?;
        ProtocolRequest {
            schema_version: 1,
            request_id: None,
            method,
            params: value,
        }
    };
    if let Some(method) = positional_method {
        if request.method != method {
            return Err(format!(
                "positional method {} does not match request method {}",
                method, request.method
            )
            .into());
        }
        request.method = method;
    }
    Ok(request)
}

fn build_protocol_request_from_args(
    args: &mut Vec<String>,
    method: Option<MethodName>,
) -> Result<ProtocolRequest<Value>, Box<dyn std::error::Error>> {
    let method =
        method.ok_or("protocol command requires a method, for example memory.queryViewport")?;
    let params = match method {
        MethodName::Status
        | MethodName::Describe
        | MethodName::SearchSemantic
        | MethodName::Subscribe => {
            ensure_no_args(args)?;
            json!({})
        }
        MethodName::QueryDensity => serde_json::to_value(parse_query_density_request(args)?)?,
        MethodName::HydrateObjects => serde_json::to_value(parse_hydrate_objects_request(args)?)?,
        MethodName::QueryViewport => {
            let options = QueryViewportOptions::parse(args)?;
            json!({
                "user_id": options.user_id.map(|value| value.to_string()).unwrap_or_default(),
                "time": {
                    "start": options.start.to_rfc3339_opts(SecondsFormat::Millis, true),
                    "end": options.end.to_rfc3339_opts(SecondsFormat::Millis, true),
                },
                "filters": viewport_source_filter(options.source.as_deref()),
                "pagination": {
                    "limit": options.limit,
                    "cursor": null,
                },
                "include": {
                    "payload": false,
                    "blob_descriptor": true,
                    "source_record": true,
                    "edges_count": true,
                },
                "explain": false,
            })
        }
        MethodName::WriteObjects
        | MethodName::IngestSources
        | MethodName::QueryEdges
        | MethodName::SearchText
        | MethodName::Explain
        | MethodName::StorageHealth
        | MethodName::StorageDiagnostics => {
            ensure_no_args(args)?;
            json!({})
        }
        MethodName::EnsureStorageReady
        | MethodName::RepairStorage
        | MethodName::StopStorage
        | MethodName::RestartStorage => {
            ensure_no_args(args)?;
            json!({"reason": "cli"})
        }
        MethodName::EnsureRecentBackfill => {
            ensure_no_args(args)?;
            json!({"reason": "cli", "window_hours": 72, "block_until_ready": false})
        }
    };
    Ok(ProtocolRequest {
        schema_version: 1,
        request_id: None,
        method,
        params,
    })
}

fn dispatch_protocol_request(
    request: ProtocolRequest<Value>,
    database: Option<DatabaseTarget>,
    elapsed_ms: u64,
) -> ProtocolResponse<Value> {
    let stats = ProtocolStats::elapsed(elapsed_ms);
    let method = request.method;
    let request_id = request.request_id;
    match method {
        MethodName::Status => ProtocolResponse::ok(
            method,
            request_id,
            serde_json::to_value(protocol_status_response()).expect("status serializes"),
            stats,
        ),
        MethodName::Describe => ProtocolResponse::ok(
            method,
            request_id,
            serde_json::to_value(describe_memory_query_protocol()).expect("describe serializes"),
            stats,
        ),
        MethodName::StorageHealth => {
            match serde_json::from_value::<onecontext_memory_db::StorageHealthRequest>(
                request.params.clone(),
            ) {
                Ok(_) => ProtocolResponse::ok(
                    method,
                    request_id,
                    serde_json::to_value(storage_health_response(database.as_ref(), false))
                        .expect("storage health serializes"),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::EnsureStorageReady => {
            match serde_json::from_value::<onecontext_memory_db::EnsureStorageReadyRequest>(
                request.params.clone(),
            ) {
                Ok(_) => ProtocolResponse::ok(
                    method,
                    request_id,
                    serde_json::to_value(storage_health_response(database.as_ref(), true))
                        .expect("storage health serializes"),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::SearchSemantic => {
            match serde_json::from_value::<search::SearchSemanticRequest>(request.params.clone()) {
                Ok(_) => ProtocolResponse::error(
                    method,
                    request_id,
                    ProtocolError::new(
                        "SEMANTIC_INDEX_NOT_READY",
                        "Semantic search is available in the protocol but no embeddings have been populated.",
                        true,
                    ),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::WriteObjects => dispatch_db_operation(
            method,
            request_id,
            request.params,
            database,
            stats,
            "WRITE_FAILED",
            |client, params| {
                let mut typed = serde_json::from_value::<
                    onecontext_memory_db::write_objects::WriteObjectsRequest,
                >(ensure_write_id(params))?;
                if typed.atomicity.is_none() {
                    typed.atomicity = Some("chunk".to_string());
                }
                Ok(serde_json::to_value(
                    onecontext_memory_db::write_objects::write_objects_with_client(client, &typed)?,
                )?)
            },
        ),
        MethodName::IngestSources => dispatch_db_operation(
            method,
            request_id,
            request.params,
            database,
            stats,
            "INGEST_FAILED",
            |client, params| {
                let typed = protocol_ingest_request_to_agent_c(params)?;
                Ok(serde_json::to_value(
                    onecontext_memory_db::ingest_sources::ingest_sources_with_client(
                        client, &typed,
                    )?,
                )?)
            },
        ),
        MethodName::QueryViewport => dispatch_db_read(
            method,
            request_id,
            request.params,
            database,
            stats,
            |client, params| {
                let typed = serde_json::from_value::<read_viewport::QueryViewportRequest>(params)?;
                Ok(serde_json::to_value(read_viewport::query_viewport(
                    client, &typed,
                )?)?)
            },
        ),
        MethodName::QueryDensity => dispatch_db_read(
            method,
            request_id,
            request.params,
            database,
            stats,
            |client, params| {
                let typed = serde_json::from_value::<query_density::QueryDensityRequest>(params)?;
                Ok(serde_json::to_value(query_density::query_density(
                    client, &typed,
                )?)?)
            },
        ),
        MethodName::HydrateObjects => dispatch_db_read(
            method,
            request_id,
            request.params,
            database,
            stats,
            |client, params| {
                let typed = serde_json::from_value::<hydrate::HydrateObjectsRequest>(params)?;
                Ok(serde_json::to_value(hydrate::hydrate_objects(
                    client, &typed,
                )?)?)
            },
        ),
        MethodName::QueryEdges => dispatch_db_read(
            method,
            request_id,
            request.params,
            database,
            stats,
            |client, params| {
                let typed = serde_json::from_value::<edges::QueryEdgesRequest>(params)?;
                Ok(serde_json::to_value(edges::query_edges(client, &typed)?)?)
            },
        ),
        MethodName::SearchText => dispatch_db_read(
            method,
            request_id,
            request.params,
            database,
            stats,
            |client, params| {
                let typed = serde_json::from_value::<search::SearchTextRequest>(params)?;
                Ok(serde_json::to_value(search::search_text(client, &typed)?)?)
            },
        ),
        MethodName::Explain => {
            dispatch_explain(method, request_id, request.params, database, stats)
        }
        MethodName::EnsureRecentBackfill => {
            match serde_json::from_value::<onecontext_memory_db::EnsureRecentBackfillRequest>(
                request.params.clone(),
            ) {
                Ok(typed) => match ensure_recent_backfill(database.as_ref(), &typed) {
                    Ok(health) => ProtocolResponse::ok(
                        method,
                        request_id,
                        serde_json::to_value(health).expect("recent backfill serializes"),
                        stats,
                    ),
                    Err(error) => ProtocolResponse::error(method, request_id, error, stats),
                },
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::RepairStorage => {
            match serde_json::from_value::<onecontext_memory_db::RepairStorageRequest>(
                request.params.clone(),
            ) {
                Ok(_) => ProtocolResponse::ok(
                    method,
                    request_id,
                    serde_json::to_value(storage_health_response(database.as_ref(), true))
                        .expect("storage health serializes"),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::StopStorage => {
            match serde_json::from_value::<onecontext_memory_db::StorageLifecycleRequest>(
                request.params.clone(),
            ) {
                Ok(_) => ProtocolResponse::ok(
                    method,
                    request_id,
                    serde_json::to_value(stop_storage_response(database.as_ref()))
                        .expect("storage health serializes"),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::RestartStorage => {
            match serde_json::from_value::<onecontext_memory_db::StorageLifecycleRequest>(
                request.params.clone(),
            ) {
                Ok(_) => ProtocolResponse::ok(
                    method,
                    request_id,
                    serde_json::to_value(restart_storage_response(database.as_ref()))
                        .expect("storage health serializes"),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::StorageDiagnostics => {
            match serde_json::from_value::<onecontext_memory_db::StorageDiagnosticsRequest>(
                request.params.clone(),
            ) {
                Ok(_) => ProtocolResponse::ok(
                    method,
                    request_id,
                    serde_json::to_value(storage_diagnostics_response(database.as_ref()))
                        .expect("storage diagnostics serializes"),
                    stats,
                ),
                Err(error) => invalid_params_response(method, request_id, error, stats),
            }
        }
        MethodName::Subscribe => typed_stub::<onecontext_memory_db::SubscribeRequest>(
            method,
            request_id,
            request.params,
            "memory.subscribe is a visible V0 stub; polling remains the V0 update path.",
            stats,
        ),
    }
}

fn typed_stub<T>(
    method: MethodName,
    request_id: Option<String>,
    params: Value,
    message: &str,
    stats: ProtocolStats,
) -> ProtocolResponse<Value>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let message: String = message.into();
    match serde_json::from_value::<T>(params) {
        Ok(typed) => ProtocolResponse::error(
            method,
            request_id,
            ProtocolError::new("METHOD_NOT_IMPLEMENTED", message, false).with_details(json!({
                "method": method.as_str(),
                "state": method.state(),
                "request": serde_json::to_value(&typed).expect("typed request serializes"),
            })),
            stats,
        ),
        Err(error) => invalid_params_response(method, request_id, error, stats),
    }
}

fn ensure_write_id(mut params: Value) -> Value {
    if let Some(object) = params.as_object_mut() {
        let needs_write_id = object
            .get("write_id")
            .is_none_or(|value| value.is_null() || value.as_str().is_some_and(str::is_empty));
        if needs_write_id {
            object.insert("write_id".to_string(), json!(generated_protocol_write_id()));
        }
    }
    params
}

fn generated_protocol_write_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let low_bits = nanos & 0x0fff_ffff_ffff_ffff_ffff_ffff_ffff_ffff;
    uuid::Uuid::from_u128(0x3000_0000_0000_0000_0000_0000_0000_0000 | low_bits).to_string()
}

fn protocol_ingest_request_to_agent_c(
    params: Value,
) -> Result<onecontext_memory_db::ingest_sources::IngestSourcesRequest, Box<dyn std::error::Error>>
{
    let protocol = serde_json::from_value::<onecontext_memory_db::IngestSourcesRequest>(params)?;
    let session_profile = protocol
        .session_profile
        .as_deref()
        .map(parse_session_profile)
        .transpose()?
        .unwrap_or_default();
    Ok(onecontext_memory_db::ingest_sources::IngestSourcesRequest {
        user_id: protocol.user_id,
        write_id: None,
        sources: protocol.sources,
        home: protocol.home,
        max_events: protocol.max_events.unwrap_or(DEFAULT_MAX_EVENTS),
        max_lines: protocol.max_lines.unwrap_or(DEFAULT_MAX_LINES),
        include_sensitive_text: protocol.include_sensitive_text.unwrap_or(false),
        session_profile,
        cursor_name: protocol.cursor_name,
    })
}

fn dispatch_db_read<F>(
    method: MethodName,
    request_id: Option<String>,
    params: Value,
    database: Option<DatabaseTarget>,
    stats: ProtocolStats,
    read: F,
) -> ProtocolResponse<Value>
where
    F: FnOnce(&mut Client, Value) -> Result<Value, Box<dyn std::error::Error>>,
{
    dispatch_db_operation(
        method,
        request_id,
        params,
        database,
        stats,
        "READ_FAILED",
        read,
    )
}

fn dispatch_db_operation<F>(
    method: MethodName,
    request_id: Option<String>,
    params: Value,
    database: Option<DatabaseTarget>,
    stats: ProtocolStats,
    failure_code: &'static str,
    operation: F,
) -> ProtocolResponse<Value>
where
    F: FnOnce(&mut Client, Value) -> Result<Value, Box<dyn std::error::Error>>,
{
    let database = match operation_database_target(database) {
        Ok(database) => database,
        Err(error) => {
            return ProtocolResponse::error(method, request_id, error, stats);
        }
    };
    let Some(database) = database else {
        return ProtocolResponse::error(
            method,
            request_id,
            ProtocolError::new(
                "STORAGE_DISABLED",
                "Perception DB storage is disabled for this process.",
                false,
            ),
            stats,
        );
    };
    let mut client = match Client::connect(&database.url, NoTls) {
        Ok(client) => client,
        Err(error) => {
            return ProtocolResponse::error(
                method,
                request_id,
                ProtocolError::new(
                    "DATABASE_CONNECT_FAILED",
                    sanitize_database_error(&error.to_string(), &database.url),
                    true,
                ),
                stats.with("database_url_source", database.source),
            );
        }
    };
    match operation(&mut client, params) {
        Ok(result) => ProtocolResponse::ok(
            method,
            request_id,
            result,
            stats.with("database_url_source", database.source),
        ),
        Err(error) => {
            let (code, retryable) = protocol_error_code_and_retryability(&*error, failure_code);
            ProtocolResponse::error(
                method,
                request_id,
                ProtocolError::new(code, error.to_string(), retryable),
                stats.with("database_url_source", database.source),
            )
        }
    }
}

enum StorageBackendResolution {
    Backend(StorageBackend),
    Invalid(String),
}

fn storage_health_response(database: Option<&DatabaseTarget>, ensure: bool) -> StorageHealth {
    match resolve_storage_backend(database) {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres) => {
            managed_postgres_health(ensure)
        }
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres) => {
            external_postgres_health(database, ensure)
        }
        StorageBackendResolution::Backend(StorageBackend::Disabled) => disabled_storage_health(),
        StorageBackendResolution::Invalid(value) => invalid_storage_backend_health(value),
    }
}

fn stop_storage_response(database: Option<&DatabaseTarget>) -> StorageHealth {
    match resolve_storage_backend(database) {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres) => {
            stop_managed_postgres_health()
        }
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres)
        | StorageBackendResolution::Backend(StorageBackend::Disabled) => {
            storage_health_response(database, false)
        }
        StorageBackendResolution::Invalid(value) => invalid_storage_backend_health(value),
    }
}

fn restart_storage_response(database: Option<&DatabaseTarget>) -> StorageHealth {
    match resolve_storage_backend(database) {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres) => {
            managed_postgres_health(true)
        }
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres)
        | StorageBackendResolution::Backend(StorageBackend::Disabled) => {
            storage_health_response(database, false)
        }
        StorageBackendResolution::Invalid(value) => invalid_storage_backend_health(value),
    }
}

fn resolve_storage_backend(database: Option<&DatabaseTarget>) -> StorageBackendResolution {
    if let Ok(value) = std::env::var(STORAGE_BACKEND_ENV) {
        let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
        return match normalized.as_str() {
            "managed" | "managed_postgres" => {
                StorageBackendResolution::Backend(StorageBackend::ManagedPostgres)
            }
            "external" | "external_postgres" => {
                StorageBackendResolution::Backend(StorageBackend::ExternalPostgres)
            }
            "disabled" => StorageBackendResolution::Backend(StorageBackend::Disabled),
            _ => StorageBackendResolution::Invalid(value),
        };
    }

    if database.is_some() {
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres)
    } else {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres)
    }
}

fn operation_database_target(
    database: Option<DatabaseTarget>,
) -> Result<Option<DatabaseTarget>, ProtocolError> {
    match resolve_storage_backend(database.as_ref()) {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres) => {
            let runtime = ensure_managed_postgres_ready().map_err(storage_health_protocol_error)?;
            Ok(Some(DatabaseTarget {
                url: runtime.database_url,
                source: "managed_postgres".to_string(),
            }))
        }
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres) => Ok(database),
        StorageBackendResolution::Backend(StorageBackend::Disabled) => Ok(None),
        StorageBackendResolution::Invalid(value) => Err(ProtocolError::new(
            "INVALID_STORAGE_BACKEND",
            format!(
                "Invalid {STORAGE_BACKEND_ENV} value {value:?}; expected managed_postgres, external_postgres, or disabled."
            ),
            false,
        )),
    }
}

struct ManagedPgRuntime {
    manifest: onecontext_memory_db::ManagedPgManifest,
    paths: onecontext_memory_db::ManagedPgPaths,
    database_url: String,
}

struct RecentBackfillSnapshot {
    event_count: i64,
    session_count: i64,
    bucket_count: i64,
    last_successful_ingest_ts: Option<i64>,
}

fn managed_postgres_health(ensure: bool) -> StorageHealth {
    if ensure {
        return match ensure_managed_postgres_ready() {
            Ok(mut runtime) => managed_live_health(
                &mut runtime,
                recent_backfill_not_checked(),
                false,
                Some("Local managed Postgres is ready.".to_string()),
                None,
            ),
            Err(health) => health,
        };
    }

    match managed_runtime_for_existing_cluster() {
        Ok(Some(mut runtime)) => managed_live_health(
            &mut runtime,
            recent_backfill_not_checked(),
            false,
            None,
            None,
        ),
        Ok(None) => managed_probe_health(),
        Err(detail) => {
            let mut health = managed_probe_health();
            health.status = "managed_postgres_unhealthy".to_string();
            health.ready = false;
            health.storage_ready = false;
            health.message = "Managed Postgres cluster exists but is not healthy.".to_string();
            health.detail = Some(detail);
            health
        }
    }
}

fn stop_managed_postgres_health() -> StorageHealth {
    let config = onecontext_memory_db::ManagedPgPathConfig::from_env();
    let paths = config.resolve_paths();
    match stop_managed_postgres_with_config(&config) {
        Ok(_) => {
            let mut health = managed_postgres_health(false);
            health.message = "Local managed Postgres was stopped.".to_string();
            health
        }
        Err(error) => managed_runtime_error_health(
            &paths,
            "managed_postgres_stop_failed",
            "Managed Postgres could not be stopped.".to_string(),
            Some(error.to_string()),
            true,
        ),
    }
}

fn stop_managed_postgres_with_config(
    config: &onecontext_memory_db::ManagedPgPathConfig,
) -> Result<(), onecontext_memory_db::ManagedPostgresError> {
    let paths = config.resolve_paths();
    let manifest =
        onecontext_memory_db::ManagedPgManifest::load_and_validate(config.resolve_bundle_prefix())?;
    let plan = onecontext_memory_db::ManagedPostgresCommandPlan::stop(&manifest, &paths);
    let mut runner = onecontext_memory_db::StdManagedPostgresCommandRunner;
    let output = runner.run(&plan)?;
    if output.success() {
        Ok(())
    } else {
        Err(onecontext_memory_db::ManagedPostgresError::command_failed(
            plan.program,
            plan.args,
            output.status,
            output.stdout,
            output.stderr,
        ))
    }
}

fn external_postgres_health(database: Option<&DatabaseTarget>, ensure: bool) -> StorageHealth {
    let Some(database) = database else {
        return StorageHealth {
            backend: StorageBackend::ExternalPostgres,
            status: "external_postgres_unavailable".to_string(),
            ready: false,
            storage_ready: false,
            recent_history_ready: false,
            user_action_required: false,
            safe_to_retry: true,
            schema_state: None,
            schema_version: None,
            expected_schema_version: None,
            app_support_dir: None,
            pgdata_dir: None,
            socket_dir: None,
            bundle_prefix: None,
            bundle_postgres_version: None,
            bundle_timescale_version: None,
            bundle_build_id: None,
            required_extensions: required_extensions_unknown(),
            recent_backfill: recent_backfill_not_checked(),
            message: "External Postgres is selected but no database URL is configured.".to_string(),
            detail: Some(format!(
                "Set {DATABASE_URL_ENV} or pass --database-url for external_postgres mode."
            )),
        };
    };

    match Client::connect(&database.url, NoTls) {
        Ok(mut client) => {
            let mut message = "External Postgres is reachable.".to_string();
            let mut detail = Some(format!("database_url_source={}", database.source));
            let schema_state = if ensure {
                match onecontext_memory_db::bootstrap_current_schema_with_client(&mut client) {
                    Ok(_) => {
                        message = "External Postgres is reachable and current schema is ready."
                            .to_string();
                        Some("valid".to_string())
                    }
                    Err(error) => {
                        detail = Some(format!(
                            "database_url_source={}; bootstrap={error}",
                            database.source
                        ));
                        Some("missing_or_incompatible".to_string())
                    }
                }
            } else {
                match onecontext_memory_db::validate_current_schema_with_client(&mut client) {
                    Ok(_) => Some("valid".to_string()),
                    Err(_) => Some("missing_or_incompatible".to_string()),
                }
            };
            StorageHealth {
                backend: StorageBackend::ExternalPostgres,
                status: "external_postgres_ready".to_string(),
                ready: true,
                storage_ready: true,
                recent_history_ready: false,
                user_action_required: false,
                safe_to_retry: true,
                schema_state,
                schema_version: None,
                expected_schema_version: None,
                app_support_dir: None,
                pgdata_dir: None,
                socket_dir: None,
                bundle_prefix: None,
                bundle_postgres_version: None,
                bundle_timescale_version: None,
                bundle_build_id: None,
                required_extensions: inspect_required_extensions(&mut client),
                recent_backfill: recent_backfill_not_checked(),
                message,
                detail,
            }
        }
        Err(error) => StorageHealth {
            backend: StorageBackend::ExternalPostgres,
            status: "external_postgres_unavailable".to_string(),
            ready: false,
            storage_ready: false,
            recent_history_ready: false,
            user_action_required: false,
            safe_to_retry: true,
            schema_state: None,
            schema_version: None,
            expected_schema_version: None,
            app_support_dir: None,
            pgdata_dir: None,
            socket_dir: None,
            bundle_prefix: None,
            bundle_postgres_version: None,
            bundle_timescale_version: None,
            bundle_build_id: None,
            required_extensions: required_extensions_unknown(),
            recent_backfill: recent_backfill_not_checked(),
            message: "External Postgres is not reachable.".to_string(),
            detail: Some(sanitize_database_error(&error.to_string(), &database.url)),
        },
    }
}

fn disabled_storage_health() -> StorageHealth {
    StorageHealth {
        backend: StorageBackend::Disabled,
        status: "disabled".to_string(),
        ready: false,
        storage_ready: false,
        recent_history_ready: false,
        user_action_required: false,
        safe_to_retry: false,
        schema_state: None,
        schema_version: None,
        expected_schema_version: None,
        app_support_dir: None,
        pgdata_dir: None,
        socket_dir: None,
        bundle_prefix: None,
        bundle_postgres_version: None,
        bundle_timescale_version: None,
        bundle_build_id: None,
        required_extensions: Vec::new(),
        recent_backfill: recent_backfill_not_checked(),
        message: "Perception DB storage is disabled for this process.".to_string(),
        detail: Some(format!("{STORAGE_BACKEND_ENV}=disabled")),
    }
}

fn invalid_storage_backend_health(value: String) -> StorageHealth {
    StorageHealth {
        backend: StorageBackend::ManagedPostgres,
        status: "storage_backend_invalid".to_string(),
        ready: false,
        storage_ready: false,
        recent_history_ready: false,
        user_action_required: true,
        safe_to_retry: false,
        schema_state: None,
        schema_version: None,
        expected_schema_version: None,
        app_support_dir: app_support_dir_string(),
        pgdata_dir: None,
        socket_dir: None,
        bundle_prefix: None,
        bundle_postgres_version: None,
        bundle_timescale_version: None,
        bundle_build_id: None,
        required_extensions: required_extensions_unknown(),
        recent_backfill: recent_backfill_not_checked(),
        message: format!("Invalid {STORAGE_BACKEND_ENV} value."),
        detail: Some(format!(
            "got {value:?}; expected managed_postgres, external_postgres, or disabled"
        )),
    }
}

fn recent_backfill_not_checked() -> onecontext_memory_db::RecentBackfillHealth {
    recent_backfill_response(72)
}

fn recent_backfill_response(window_hours: u32) -> onecontext_memory_db::RecentBackfillHealth {
    onecontext_memory_db::RecentBackfillHealth {
        status: "not_checked".to_string(),
        window_hours,
        density_ready: false,
        event_count: None,
        session_count: None,
        last_successful_ingest_ts: None,
        message: Some(
            "Recent backfill readiness is not implemented in the PR 1 storage skeleton."
                .to_string(),
        ),
    }
}

fn managed_probe_health() -> StorageHealth {
    let probe = onecontext_memory_db::probe_managed_postgres_read_only();
    let required_extensions = probe
        .required_extensions
        .iter()
        .map(|extension| onecontext_memory_db::ExtensionHealth {
            name: extension.name.clone(),
            required: extension.required,
            installed: false,
            version: None,
            preload_required: extension.preload_required,
            preload_active: None,
        })
        .collect::<Vec<_>>();
    let detail = probe
        .detail
        .clone()
        .map(|detail| format!("{detail}; bundle_prefix={}", probe.bundle_prefix.display()))
        .or_else(|| Some(format!("bundle_prefix={}", probe.bundle_prefix.display())));
    StorageHealth {
        backend: StorageBackend::ManagedPostgres,
        status: probe.state.as_str().to_string(),
        ready: false,
        storage_ready: false,
        recent_history_ready: false,
        user_action_required: matches!(
            probe.state,
            onecontext_memory_db::ManagedPostgresReadOnlyState::BundleMissing
                | onecontext_memory_db::ManagedPostgresReadOnlyState::BundleInvalid
        ),
        safe_to_retry: probe.safe_to_retry,
        schema_state: None,
        schema_version: None,
        expected_schema_version: None,
        app_support_dir: Some(probe.app_support_dir.display().to_string()),
        pgdata_dir: Some(probe.pgdata_dir.display().to_string()),
        socket_dir: Some(probe.socket_dir.display().to_string()),
        bundle_prefix: Some(probe.bundle_prefix.display().to_string()),
        bundle_postgres_version: probe.postgres_version,
        bundle_timescale_version: probe.timescale_version,
        bundle_build_id: probe.build_id,
        required_extensions,
        recent_backfill: recent_backfill_not_checked(),
        message: probe.message,
        detail,
    }
}

fn managed_runtime_for_existing_cluster() -> Result<Option<ManagedPgRuntime>, String> {
    let config = onecontext_memory_db::ManagedPgPathConfig::from_env();
    let paths = config.resolve_paths();
    let bundle_prefix = config.resolve_bundle_prefix();
    let manifest = match onecontext_memory_db::ManagedPgManifest::load_and_validate(&bundle_prefix)
    {
        Ok(manifest) => manifest,
        Err(_) => return Ok(None),
    };
    if !paths.pgdata.join("PG_VERSION").is_file() {
        return Ok(None);
    }
    let database_url =
        onecontext_memory_db::ManagedPostgresConfigPlan::for_paths(&paths).database_url();
    Ok(Some(ManagedPgRuntime {
        manifest,
        paths,
        database_url,
    }))
}

fn ensure_managed_postgres_ready() -> Result<ManagedPgRuntime, StorageHealth> {
    let config = onecontext_memory_db::ManagedPgPathConfig::from_env();
    let paths = config.resolve_paths();
    let bundle_prefix = config.resolve_bundle_prefix();
    let ready = match onecontext_memory_db::ensure_managed_postgres_ready_with_config(&config) {
        Ok(ready) => ready,
        Err(error) => {
            if matches!(
                error,
                onecontext_memory_db::ManagedPostgresError::BundleMissing { .. }
                    | onecontext_memory_db::ManagedPostgresError::BundleInvalid { .. }
                    | onecontext_memory_db::ManagedPostgresError::InvalidManifest { .. }
                    | onecontext_memory_db::ManagedPostgresError::UnsupportedArch { .. }
            ) {
                return Err(managed_probe_health());
            }
            return Err(managed_runtime_error_health(
                &paths,
                "managed_postgres_start_failed",
                "Managed Postgres did not become ready.".to_string(),
                Some(error.to_string()),
                true,
            ));
        }
    };
    let manifest = match onecontext_memory_db::ManagedPgManifest::load_and_validate(&bundle_prefix)
    {
        Ok(manifest) => manifest,
        Err(error) => {
            return Err(managed_runtime_error_health(
                &paths,
                "managed_postgres_bundle_invalid",
                "Managed Postgres became ready but the bundle manifest could not be reloaded."
                    .to_string(),
                Some(error.to_string()),
                false,
            ));
        }
    };
    Ok(ManagedPgRuntime {
        manifest,
        paths,
        database_url: ready.database_url,
    })
}

fn managed_live_health(
    runtime: &mut ManagedPgRuntime,
    recent_backfill: onecontext_memory_db::RecentBackfillHealth,
    recent_history_ready: bool,
    message_override: Option<String>,
    detail_override: Option<String>,
) -> StorageHealth {
    match Client::connect(&runtime.database_url, NoTls) {
        Ok(mut client) => {
            let schema_state =
                match onecontext_memory_db::validate_current_schema_with_client(&mut client) {
                    Ok(_) => Some("valid".to_string()),
                    Err(_) => Some("missing_or_incompatible".to_string()),
                };
            StorageHealth {
                backend: StorageBackend::ManagedPostgres,
                status: "ready".to_string(),
                ready: true,
                storage_ready: true,
                recent_history_ready,
                user_action_required: false,
                safe_to_retry: true,
                schema_state,
                schema_version: Some(onecontext_memory_db::BASELINE_SCHEMA_VERSION),
                expected_schema_version: Some(onecontext_memory_db::BASELINE_SCHEMA_VERSION),
                app_support_dir: Some(runtime.paths.app_support.display().to_string()),
                pgdata_dir: Some(runtime.paths.pgdata.display().to_string()),
                socket_dir: Some(runtime.paths.socket_dir.display().to_string()),
                bundle_prefix: Some(runtime.manifest.prefix.display().to_string()),
                bundle_postgres_version: Some(runtime.manifest.postgres_version.clone()),
                bundle_timescale_version: Some(runtime.manifest.timescale_version.clone()),
                bundle_build_id: Some(runtime.manifest.build_id.clone()),
                required_extensions: inspect_required_extensions(&mut client),
                recent_backfill,
                message: message_override
                    .unwrap_or_else(|| "Local managed Postgres is reachable.".to_string()),
                detail: detail_override.or_else(|| {
                    Some(format!(
                        "bundle_prefix={}; database_url_source=managed_postgres",
                        runtime.manifest.prefix.display()
                    ))
                }),
            }
        }
        Err(error) => managed_runtime_error_health(
            &runtime.paths,
            "managed_postgres_connect_failed",
            "Managed Postgres cluster exists but the application connection failed.".to_string(),
            Some(error.to_string()),
            true,
        ),
    }
}

fn managed_runtime_error_health(
    paths: &onecontext_memory_db::ManagedPgPaths,
    status: &str,
    message: String,
    detail: Option<String>,
    safe_to_retry: bool,
) -> StorageHealth {
    StorageHealth {
        backend: StorageBackend::ManagedPostgres,
        status: status.to_string(),
        ready: false,
        storage_ready: false,
        recent_history_ready: false,
        user_action_required: false,
        safe_to_retry,
        schema_state: None,
        schema_version: None,
        expected_schema_version: Some(onecontext_memory_db::BASELINE_SCHEMA_VERSION),
        app_support_dir: Some(paths.app_support.display().to_string()),
        pgdata_dir: Some(paths.pgdata.display().to_string()),
        socket_dir: Some(paths.socket_dir.display().to_string()),
        bundle_prefix: None,
        bundle_postgres_version: None,
        bundle_timescale_version: None,
        bundle_build_id: None,
        required_extensions: required_extensions_unknown(),
        recent_backfill: recent_backfill_not_checked(),
        message,
        detail,
    }
}

fn ensure_recent_backfill(
    database: Option<&DatabaseTarget>,
    request: &onecontext_memory_db::EnsureRecentBackfillRequest,
) -> Result<onecontext_memory_db::RecentBackfillHealth, ProtocolError> {
    match resolve_storage_backend(database) {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres) => {
            let runtime = ensure_managed_postgres_ready().map_err(storage_health_protocol_error)?;
            let mut client = Client::connect(&runtime.database_url, NoTls).map_err(|error| {
                ProtocolError::new(
                    "DATABASE_CONNECT_FAILED",
                    sanitize_database_error(&error.to_string(), &runtime.database_url),
                    true,
                )
            })?;
            run_recent_backfill_with_client(
                &mut client,
                request,
                managed_readable_sources(default_home_dir()),
            )
        }
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres) => {
            let Some(database) = database else {
                return Err(ProtocolError::new(
                    "DATABASE_UNCONFIGURED",
                    "External Postgres backfill requires ONECONTEXT_MEMORY_DB_URL or --database-url.",
                    false,
                ));
            };
            let mut client = Client::connect(&database.url, NoTls).map_err(|error| {
                ProtocolError::new(
                    "DATABASE_CONNECT_FAILED",
                    sanitize_database_error(&error.to_string(), &database.url),
                    true,
                )
            })?;
            run_recent_backfill_with_client(
                &mut client,
                request,
                managed_readable_sources(default_home_dir()),
            )
        }
        StorageBackendResolution::Backend(StorageBackend::Disabled) => {
            Err(ProtocolError::new(
                "STORAGE_DISABLED",
                "Perception DB storage is disabled for this process.",
                false,
            ))
        }
        StorageBackendResolution::Invalid(value) => Err(ProtocolError::new(
            "INVALID_STORAGE_BACKEND",
            format!(
                "Invalid {STORAGE_BACKEND_ENV} value {value:?}; expected managed_postgres, external_postgres, or disabled."
            ),
            false,
        )),
    }
}

fn run_recent_backfill_with_client(
    client: &mut Client,
    request: &onecontext_memory_db::EnsureRecentBackfillRequest,
    sources: Vec<String>,
) -> Result<onecontext_memory_db::RecentBackfillHealth, ProtocolError> {
    if sources.is_empty() {
        return Ok(onecontext_memory_db::RecentBackfillHealth {
            status: "no_sources".to_string(),
            window_hours: request.window_hours,
            density_ready: false,
            event_count: Some(0),
            session_count: Some(0),
            last_successful_ingest_ts: None,
            message: Some("No readable local sources were found for recent backfill.".to_string()),
        });
    }

    let mut last_result = None;
    let attempts = if request.block_until_ready { 3 } else { 1 };
    for _ in 0..attempts {
        let ingest = onecontext_memory_db::ingest_sources::ingest_sources_with_client(
            client,
            &onecontext_memory_db::ingest_sources::IngestSourcesRequest {
                user_id: LOCAL_USER_ID.to_string(),
                write_id: None,
                sources: sources.clone(),
                home: Some(default_home_dir()),
                max_events: DEFAULT_MAX_EVENTS,
                max_lines: DEFAULT_MAX_LINES,
                include_sensitive_text: false,
                session_profile: SessionIngestProfile::default(),
                cursor_name: Some(RECENT_BACKFILL_CURSOR_NAME.to_string()),
            },
        )
        .map_err(|error| ProtocolError::new("INGEST_FAILED", error.to_string(), true))?;
        let refresh_start = Utc::now() - ChronoDuration::hours(request.window_hours.max(1) as i64);
        refresh_density_window(client, refresh_start, Utc::now()).map_err(|error| {
            ProtocolError::new("DENSITY_REFRESH_FAILED", error.to_string(), true)
        })?;
        let snapshot = recent_backfill_snapshot(client, request.window_hours)
            .map_err(|error| ProtocolError::new("READ_FAILED", error.to_string(), true))?;
        let health = recent_backfill_health_from_snapshot(request, &ingest, snapshot);
        let density_ready = health.density_ready;
        last_result = Some(health);
        if density_ready {
            break;
        }
    }
    Ok(last_result.unwrap_or_else(|| recent_backfill_response(request.window_hours)))
}

fn managed_readable_sources(home: PathBuf) -> Vec<String> {
    onecontext_memory_db::probe_local_sources(&home)
        .into_iter()
        .filter(|report| report.readable)
        .filter_map(|report| match report.connector_key.as_str() {
            "codex.local_sessions" => Some("codex".to_string()),
            "claude.local_sessions" => Some("claude".to_string()),
            "imessage.chat_db" => Some("imessage".to_string()),
            _ => None,
        })
        .collect()
}

fn refresh_density_window(
    client: &mut Client,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(), postgres::Error> {
    client.batch_execute(&format!(
        "CALL refresh_continuous_aggregate('perception.object_density_1m', '{}'::timestamptz, '{}'::timestamptz);",
        start.to_rfc3339_opts(SecondsFormat::Micros, true),
        end.to_rfc3339_opts(SecondsFormat::Micros, true),
    ))
}

fn recent_backfill_snapshot(
    client: &mut Client,
    window_hours: u32,
) -> Result<RecentBackfillSnapshot, postgres::Error> {
    let end = Utc::now();
    let start = end - ChronoDuration::hours(window_hours.max(1) as i64);
    let counts = client.query_one(
        r#"
        SELECT
          count(*)::bigint AS event_count,
          count(DISTINCT series_id)::bigint AS session_count
        FROM perception.objects
        WHERE user_id = $1::uuid
          AND valid_to IS NULL
          AND event_start >= $2
          AND event_start < $3
        "#,
        &[
            &uuid::Uuid::parse_str(LOCAL_USER_ID).expect("static local user id"),
            &start,
            &end,
        ],
    )?;
    let buckets = client.query_one(
        r#"
        SELECT count(*)::bigint AS bucket_count
        FROM perception.object_density_1m
        WHERE user_id = $1::uuid
          AND bucket_start >= $2
          AND bucket_start < $3
        "#,
        &[
            &uuid::Uuid::parse_str(LOCAL_USER_ID).expect("static local user id"),
            &start,
            &end,
        ],
    )?;
    let cursors = client.query_one(
        r#"
        SELECT
          max((extract(epoch FROM advanced_at) * 1000)::bigint) AS last_successful_ingest_ts
        FROM perception.source_cursors
        WHERE user_id = $1::uuid
          AND cursor_name = $2
        "#,
        &[
            &uuid::Uuid::parse_str(LOCAL_USER_ID).expect("static local user id"),
            &RECENT_BACKFILL_CURSOR_NAME,
        ],
    )?;
    Ok(RecentBackfillSnapshot {
        event_count: counts.get("event_count"),
        session_count: counts.get("session_count"),
        bucket_count: buckets.get("bucket_count"),
        last_successful_ingest_ts: cursors.get("last_successful_ingest_ts"),
    })
}

fn recent_backfill_health_from_snapshot(
    request: &onecontext_memory_db::EnsureRecentBackfillRequest,
    ingest: &onecontext_memory_db::ingest_sources::IngestSourcesResponse,
    snapshot: RecentBackfillSnapshot,
) -> onecontext_memory_db::RecentBackfillHealth {
    let min_density_buckets = request.min_density_buckets.unwrap_or(1).max(1) as i64;
    let density_ready = snapshot.event_count > 0 && snapshot.bucket_count >= min_density_buckets;
    let error_count = ingest
        .source_results
        .iter()
        .filter(|result| result.status != "ok")
        .count();
    let status = if error_count > 0 {
        "ingest_error"
    } else if density_ready {
        "ready"
    } else if snapshot.event_count == 0 {
        "empty"
    } else {
        "density_pending"
    };
    let message = if error_count > 0 {
        let failed_sources = ingest
            .source_results
            .iter()
            .filter(|result| result.status != "ok")
            .map(|result| format!("{}:{}", result.source, result.status))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Recent backfill saw source errors: {failed_sources}"
        ))
    } else if density_ready {
        Some("Recent local memory is ready.".to_string())
    } else if snapshot.event_count == 0 {
        Some("Recent backfill found no events in the requested window.".to_string())
    } else {
        Some(format!(
            "Recent backfill wrote events, but density is still below the {} bucket threshold.",
            min_density_buckets
        ))
    };
    onecontext_memory_db::RecentBackfillHealth {
        status: status.to_string(),
        window_hours: request.window_hours,
        density_ready,
        event_count: Some(snapshot.event_count),
        session_count: Some(snapshot.session_count),
        last_successful_ingest_ts: snapshot.last_successful_ingest_ts,
        message,
    }
}

fn storage_health_protocol_error(health: StorageHealth) -> ProtocolError {
    ProtocolError::new("STORAGE_NOT_READY", health.message, health.safe_to_retry).with_details(
        json!({
            "backend": health.backend,
            "status": health.status,
            "detail": health.detail,
            "app_support_dir": health.app_support_dir,
            "pgdata_dir": health.pgdata_dir,
            "socket_dir": health.socket_dir,
        }),
    )
}

fn storage_diagnostics_response(
    database: Option<&DatabaseTarget>,
) -> onecontext_memory_db::StorageDiagnostics {
    let health = storage_health_response(database, false);
    onecontext_memory_db::StorageDiagnostics {
        ok: true,
        status: health.status,
        message: health.message,
    }
}

fn required_extensions_unknown() -> Vec<onecontext_memory_db::ExtensionHealth> {
    REQUIRED_EXTENSIONS
        .iter()
        .map(
            |(name, preload_required)| onecontext_memory_db::ExtensionHealth {
                name: (*name).to_string(),
                required: true,
                installed: false,
                version: None,
                preload_required: *preload_required,
                preload_active: None,
            },
        )
        .collect()
}

fn inspect_required_extensions(client: &mut Client) -> Vec<onecontext_memory_db::ExtensionHealth> {
    let installed = client
        .query("SELECT extname, extversion FROM pg_extension;", &[])
        .map(|rows| {
            rows.into_iter()
                .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let preload_libraries = inspect_preload_libraries(client);

    REQUIRED_EXTENSIONS
        .iter()
        .map(|(name, preload_required)| {
            let version = installed
                .iter()
                .find(|(installed_name, _)| installed_name == name)
                .map(|(_, version)| version.clone());
            onecontext_memory_db::ExtensionHealth {
                name: (*name).to_string(),
                required: true,
                installed: version.is_some(),
                version,
                preload_required: *preload_required,
                preload_active: preload_active_for(
                    *preload_required,
                    name,
                    preload_libraries.as_deref(),
                )
                .or_else(|| preload_usage_probe_for(client, *preload_required, name)),
            }
        })
        .collect()
}

fn inspect_preload_libraries(client: &mut Client) -> Option<String> {
    client
        .query_one("SHOW shared_preload_libraries;", &[])
        .ok()
        .map(|row| row.get::<_, String>(0))
        .or_else(|| {
            client
                .query_one(
                    "SELECT current_setting('shared_preload_libraries', true);",
                    &[],
                )
                .ok()
                .map(|row| row.get::<_, String>(0))
        })
}

fn preload_active_for(
    preload_required: bool,
    extension_name: &str,
    preload_libraries: Option<&str>,
) -> Option<bool> {
    if !preload_required {
        return None;
    }
    preload_libraries.map(|libraries| {
        libraries
            .split(',')
            .any(|library| library.trim().eq_ignore_ascii_case(extension_name))
    })
}

fn preload_usage_probe_for(
    client: &mut Client,
    preload_required: bool,
    extension_name: &str,
) -> Option<bool> {
    if !preload_required {
        return None;
    }
    let query = match extension_name {
        "timescaledb" => "SELECT count(*) >= 0 FROM timescaledb_information.hypertables;",
        "pg_stat_statements" => "SELECT count(*) >= 0 FROM pg_stat_statements;",
        _ => return None,
    };
    Some(client.query_one(query, &[]).is_ok())
}

fn app_support_dir_string() -> Option<String> {
    if let Ok(value) = std::env::var(APP_SUPPORT_DIR_ENV) {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }
    Some(
        default_home_dir()
            .join("Library/Application Support/1Context")
            .display()
            .to_string(),
    )
}

fn protocol_error_code_and_retryability(
    error: &(dyn std::error::Error + 'static),
    fallback_code: &'static str,
) -> (&'static str, bool) {
    if let Some(write_error) =
        error.downcast_ref::<onecontext_memory_db::write_objects::WriteObjectsError>()
    {
        let code = write_error.code();
        return (code, code == "DB_WRITE_FAILED");
    }
    (fallback_code, false)
}

fn dispatch_explain(
    method: MethodName,
    request_id: Option<String>,
    params: Value,
    database: Option<DatabaseTarget>,
    stats: ProtocolStats,
) -> ProtocolResponse<Value> {
    dispatch_db_read(
        method,
        request_id,
        params,
        database,
        stats,
        |client, params| {
            let request = serde_json::from_value::<onecontext_memory_db::ExplainRequest>(params)?;
            let target = request.target.parse::<MethodName>()?;
            let plan = match target {
                MethodName::QueryViewport => {
                    let typed = serde_json::from_value::<read_viewport::QueryViewportRequest>(
                        request.params,
                    )?;
                    read_viewport::explain_viewport(client, &typed, request.include_raw_plan)?
                }
                MethodName::QueryDensity => {
                    let typed = serde_json::from_value::<query_density::QueryDensityRequest>(
                        request.params,
                    )?;
                    query_density::explain_density(client, &typed, request.include_raw_plan)?
                }
                MethodName::SearchText => {
                    let typed =
                        serde_json::from_value::<search::SearchTextRequest>(request.params)?;
                    search::explain_search_text(client, &typed, request.include_raw_plan)?
                }
                other => {
                    return Err(format!(
                        "memory.explain target {} is not owned by Agent D read methods",
                        other
                    )
                    .into());
                }
            };
            let sql_kind = plan
                .summary
                .get("sql_kind")
                .and_then(Value::as_str)
                .unwrap_or(target.short_name())
                .to_string();
            Ok(json!({
                "ok": true,
                "target": target.short_name(),
                "sql_kind": sql_kind,
                "plan": plan,
            }))
        },
    )
}

fn invalid_params_response(
    method: MethodName,
    request_id: Option<String>,
    error: serde_json::Error,
    stats: ProtocolStats,
) -> ProtocolResponse<Value> {
    ProtocolResponse::error(
        method,
        request_id,
        ProtocolError::new(
            "INVALID_PARAMS",
            format!(
                "Request params did not match {}: {error}",
                method.request_type()
            ),
            false,
        ),
        stats,
    )
}

fn protocol_status_response() -> StatusResponse {
    let database = resolve_database_target(&mut Vec::new());
    let configured = !matches!(
        resolve_storage_backend(database.as_ref()),
        StorageBackendResolution::Backend(StorageBackend::Disabled)
            | StorageBackendResolution::Invalid(_)
    );
    StatusResponse {
        ok: true,
        service: "onecontext-memoryd".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: StorageStatus {
            configured,
            reachable: None,
            url_source: database.map(|target| target.source),
            schema_state: None,
        },
        enabled_sources: DEFAULT_SOURCES
            .split(',')
            .map(ToString::to_string)
            .collect(),
    }
}

fn run_query(args: &mut Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let query = args
        .first()
        .cloned()
        .ok_or("query command requires a query name, for example viewport")?;
    args.remove(0);
    match query.as_str() {
        "viewport" => run_query_viewport(QueryViewportOptions::parse(args)?),
        "density" => run_query_density(parse_query_density_request(args)?),
        "hydrate" | "objects" => run_hydrate_objects(parse_hydrate_objects_request(args)?),
        other => Err(format!("unsupported query {other:?}").into()),
    }
}

#[derive(Debug, Clone)]
struct QueryViewportOptions {
    database: Option<DatabaseTarget>,
    user_id: Option<uuid::Uuid>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    source: Option<String>,
    limit: i64,
}

impl QueryViewportOptions {
    fn parse(args: &mut Vec<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let now = Utc::now();
        let end = take_option_value(args, "--end")
            .or_else(|| take_option_value(args, "--end-time"))
            .map(|value| parse_rfc3339_utc("--end", &value))
            .transpose()?
            .unwrap_or(now);
        let start = take_option_value(args, "--start")
            .or_else(|| take_option_value(args, "--start-time"))
            .map(|value| parse_rfc3339_utc("--start", &value))
            .transpose()?
            .unwrap_or_else(|| end - ChronoDuration::days(7));
        let limit = take_option_value(args, "--limit")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(500)
            .clamp(1, 5_000);
        let source = take_option_value(args, "--source")
            .or_else(|| take_option_value(args, "--sources"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != "all");
        let user_id = take_option_value(args, "--user-id")
            .map(|value| {
                uuid::Uuid::parse_str(value.trim())
                    .map_err(|error| format!("invalid --user-id {value:?}: {error}"))
            })
            .transpose()?;
        let database = resolve_database_target(args);
        ensure_no_args(args)?;
        Ok(Self {
            database,
            user_id,
            start,
            end,
            source,
            limit,
        })
    }
}

fn run_query_viewport(options: QueryViewportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let Some(database) =
        operation_database_target(options.database.clone()).map_err(|error| error.message)?
    else {
        print_json(&json!({
            "ok": false,
            "error": {
                "code": "STORAGE_DISABLED",
                "message": "queryViewport requires Perception DB storage to be enabled."
            }
        }))?;
        return Ok(());
    };

    let mut client = Client::connect(&database.url, NoTls)?;
    let request = viewport_request_from_options(&options)?;
    let response = read_viewport::query_viewport(&mut client, &request)?;
    print_json(&response)?;
    Ok(())
}

fn viewport_request_from_options(
    options: &QueryViewportOptions,
) -> Result<read_viewport::QueryViewportRequest, Box<dyn std::error::Error>> {
    let user_id = options
        .user_id
        .ok_or("queryViewport requires --user-id UUID")?
        .to_string();
    let mut source_ids = Vec::new();
    let mut source_types = Vec::new();
    let mut source_keys = Vec::new();
    if let Some(source) = &options.source {
        if uuid::Uuid::parse_str(source).is_ok() {
            source_ids.push(source.clone());
        } else if source.contains('.') {
            source_keys.push(source.clone());
        } else {
            source_types.push(source.clone());
        }
    }
    Ok(read_viewport::QueryViewportRequest {
        user_id,
        time: Some(read_viewport::TimeRangeRequest {
            start: options.start.to_rfc3339_opts(SecondsFormat::Millis, true),
            end: options.end.to_rfc3339_opts(SecondsFormat::Millis, true),
        }),
        filters: read_viewport::ObjectFiltersRequest {
            source_ids,
            source_types,
            source_keys,
            ..read_viewport::ObjectFiltersRequest::default()
        },
        pagination: read_viewport::PaginationRequest {
            limit: options.limit,
            cursor: None,
        },
        include: read_viewport::ViewportIncludeRequest::default(),
        explain: false,
    })
}

fn viewport_source_filter(source: Option<&str>) -> Value {
    let Some(source) = source.filter(|value| !value.trim().is_empty()) else {
        return json!({});
    };
    if uuid::Uuid::parse_str(source).is_ok() {
        json!({"source_ids": [source]})
    } else if source.contains('.') {
        json!({"source_keys": [source]})
    } else {
        json!({"source_types": [source]})
    }
}

fn run_query_density(
    request: query_density::QueryDensityRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(database) = operation_database_target(resolve_database_target(&mut Vec::new()))
        .map_err(|error| error.message)?
    else {
        return print_json(&json!({
            "ok": false,
            "error": {
                "code": "STORAGE_DISABLED",
                "message": "queryDensity requires Perception DB storage to be enabled."
            }
        }));
    };
    let mut client = Client::connect(&database.url, NoTls)?;
    let response = query_density::query_density(&mut client, &request)?;
    print_json(&response)
}

fn run_hydrate_objects(
    request: hydrate::HydrateObjectsRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(database) = operation_database_target(resolve_database_target(&mut Vec::new()))
        .map_err(|error| error.message)?
    else {
        return print_json(&json!({
            "ok": false,
            "error": {
                "code": "STORAGE_DISABLED",
                "message": "hydrateObjects requires Perception DB storage to be enabled."
            }
        }));
    };
    let mut client = Client::connect(&database.url, NoTls)?;
    let response = hydrate::hydrate_objects(&mut client, &request)?;
    print_json(&response)
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn run_ingest_tick(options: &DaemonOptions) -> Result<Value, Box<dyn std::error::Error>> {
    let mut cursors = load_cursors(&options.cursor_file())?;
    let started = Instant::now();
    let mut source_results = Vec::new();
    let mut perception_objects = Vec::new();
    let mut user_id_hint = None;
    for source in &options.sources {
        let tick = timed_ingest_source(options, source, &mut cursors);
        source_results.push(tick.summary);
        if user_id_hint.is_none() {
            user_id_hint = tick.user_id;
        }
        perception_objects.extend(tick.perception_objects);
    }
    let db_write = write_database_batch(options, &perception_objects, user_id_hint);
    if let Err(error) = ensure_durable_before_cursor_advance(&db_write, perception_objects.len()) {
        return Ok(ingest_tick_payload(
            options,
            started.elapsed().as_millis() as u64,
            source_results,
            db_write,
            "error",
            false,
            Some(error.to_string()),
        ));
    }
    save_cursors(&options.cursor_file(), &cursors)?;
    Ok(ingest_tick_payload(
        options,
        started.elapsed().as_millis() as u64,
        source_results,
        db_write,
        "ok",
        true,
        None,
    ))
}

fn ingest_tick_payload(
    options: &DaemonOptions,
    elapsed_ms: u64,
    source_results: Vec<Value>,
    db_write: Value,
    status: &str,
    cursor_saved: bool,
    error: Option<String>,
) -> Value {
    let objects_emitted = source_results
        .iter()
        .filter_map(|source| source.get("objects_emitted").and_then(Value::as_u64))
        .sum::<u64>();
    json!({
        "status": status,
        "schema_version": 1,
        "service": "onecontext-memoryd",
        "pid": std::process::id(),
        "created_at": now(),
        "elapsed_ms": elapsed_ms,
        "objects_emitted": objects_emitted,
        "cursor_file": options.cursor_file(),
        "cursor_saved": cursor_saved,
        "status_file": options.status_file(),
        "database": database_payload(options),
        "db_write": db_write,
        "storage_posture": storage_posture_payload(options),
        "error": error,
        "options": options_payload(options),
        "sources": source_results,
    })
}

struct SourceIngestTick {
    summary: Value,
    perception_objects: Vec<onecontext_memory_db::write_objects::PerceptionObjectInput>,
    user_id: Option<String>,
}

fn timed_ingest_source(
    options: &DaemonOptions,
    source: &str,
    cursors: &mut LocalIngestCursors,
) -> SourceIngestTick {
    let started = Instant::now();
    let result = ingest_source(options, source, cursors);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(batch) => {
            let summary = batch_summary(source, elapsed_ms, &batch);
            SourceIngestTick {
                summary,
                user_id: user_id_for_batch(source, &batch),
                perception_objects: batch.perception_objects,
            }
        }
        Err(error) => SourceIngestTick {
            summary: json!({
                "source": source,
                "status": "error",
                "elapsed_ms": elapsed_ms,
                "error": error.to_string(),
            }),
            perception_objects: Vec::new(),
            user_id: default_context_for_source(source)
                .ok()
                .map(|context| context.user_id),
        },
    }
}

fn ingest_source(
    options: &DaemonOptions,
    source: &str,
    cursors: &mut LocalIngestCursors,
) -> Result<IncrementalIngestBatch, Box<dyn std::error::Error>> {
    let context = default_context_for_source(source)?;
    let ingest_options = options.ingest_options();
    Ok(match source {
        "codex" => ingest_codex_incremental(&options.home, &context, &ingest_options, cursors)?,
        "claude" => ingest_claude_incremental(&options.home, &context, &ingest_options, cursors)?,
        "imessage" => {
            ingest_imessage_incremental(&options.home, &context, &ingest_options, cursors)?
        }
        other => return Err(format!("unsupported source {other:?}").into()),
    })
}

fn batch_summary(source: &str, elapsed_ms: u64, batch: &IncrementalIngestBatch) -> Value {
    let mut object_kinds = std::collections::BTreeMap::<String, usize>::new();
    for object in &batch.perception_objects {
        *object_kinds.entry(object.kind.clone()).or_default() += 1;
    }
    json!({
        "source": source,
        "status": "ok",
        "elapsed_ms": elapsed_ms,
        "invalid_count": 0,
        "object_count": batch.perception_objects.len(),
        "objects_emitted": batch.perception_objects.len(),
        "object_kinds": object_kinds,
        "report": batch.report,
    })
}

fn user_id_for_batch(source: &str, batch: &IncrementalIngestBatch) -> Option<String> {
    let _ = batch;
    default_context_for_source(source)
        .ok()
        .map(|context| context.user_id)
}

fn write_status(
    options: &DaemonOptions,
    state: &str,
    payload: Option<Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = payload.unwrap_or_else(|| {
        json!({
            "status": state,
            "schema_version": 1,
            "service": "onecontext-memoryd",
            "pid": std::process::id(),
            "created_at": now(),
            "cursor_file": options.cursor_file(),
            "status_file": options.status_file(),
            "database": database_payload(options),
            "storage_posture": storage_posture_payload(options),
            "options": options_payload(options),
        })
    });
    fs::write(options.status_file(), serde_json::to_vec_pretty(&status)?)?;
    Ok(())
}

fn options_payload(options: &DaemonOptions) -> Value {
    json!({
        "home": options.home,
        "context_engine_root": options.context_engine_root,
        "run_dir": options.run_dir,
        "database": database_payload(options),
        "interval_ms": options.interval_ms,
        "max_events": options.max_events,
        "max_lines": options.max_lines,
        "sources": options.sources,
        "session_profile": options.session_profile,
        "include_sensitive_text": options.include_sensitive_text,
    })
}

fn write_database_batch(
    options: &DaemonOptions,
    perception_objects: &[onecontext_memory_db::write_objects::PerceptionObjectInput],
    user_id_hint: Option<String>,
) -> Value {
    if perception_objects.is_empty() {
        return json!({
            "status": "skipped",
            "attempted": false,
            "objects_seen": 0,
            "objects_written": 0,
            "objects_failed": 0,
            "elapsed_ms": 0,
            "failure_posture": database_failure_posture(options),
        });
    }
    let database = match operation_database_target(options.database.clone()) {
        Ok(database) => database,
        Err(error) => {
            return json!({
                "status": "error",
                "attempted": false,
                "write_mode": write_mode(perception_objects),
                "objects_seen": perception_objects.len(),
                "objects_written": 0,
                "objects_failed": perception_objects.len(),
                "elapsed_ms": 0,
                "error": error.message,
                "failure_posture": database_failure_posture(options),
            });
        }
    };
    let Some(database) = database else {
        return json!({
            "status": "disabled",
            "attempted": false,
            "write_mode": write_mode(perception_objects),
            "objects_seen": perception_objects.len(),
            "objects_written": 0,
            "objects_failed": 0,
            "elapsed_ms": 0,
            "failure_posture": database_failure_posture(options),
        });
    };

    let started = Instant::now();
    let object_inputs = perception_objects.to_vec();
    let user_id =
        user_id_hint.unwrap_or_else(|| "00000000-0000-0000-0000-000000000001".to_string());
    let write_id = match onecontext_memory_db::write_objects::deterministic_object_id(
        &object_inputs[0].source_id,
        &format!("memoryd-write/{}", object_inputs[0].source_record_key),
    ) {
        Ok(write_id) => write_id,
        Err(error) => {
            return json!({
                "status": "error",
                "attempted": false,
                "write_mode": write_mode(perception_objects),
                "objects_seen": perception_objects.len(),
                "objects_written": 0,
                "objects_failed": object_inputs.len(),
                "elapsed_ms": started.elapsed().as_millis() as u64,
                "database_url": redact_database_url(&database.url),
                "database_url_source": database.source,
                "writer": "write_objects",
                "error": sanitize_database_error(&error.to_string(), &database.url),
                "failure_posture": database_failure_posture(options),
            });
        }
    };
    let request = onecontext_memory_db::write_objects::WriteObjectsRequest {
        user_id,
        write_id,
        atomicity: Some("chunk".to_string()),
        records: object_inputs,
        chunk_size: None,
    };

    let mut client = match Client::connect(&database.url, NoTls) {
        Ok(client) => client,
        Err(error) => {
            return json!({
                "status": "error",
                "attempted": true,
                "write_mode": write_mode(perception_objects),
                "objects_seen": perception_objects.len(),
                "objects_written": 0,
                "objects_failed": request.records.len(),
                "elapsed_ms": started.elapsed().as_millis() as u64,
                "database_url": redact_database_url(&database.url),
                "database_url_source": database.source,
                "writer": "write_objects",
                "error": sanitize_database_error(&error.to_string(), &database.url),
                "failure_posture": database_failure_posture(options),
            });
        }
    };
    match onecontext_memory_db::write_objects::write_objects_with_client(&mut client, &request) {
        Ok(response) => {
            let inserted = response.inserted_count;
            let deduplicated = response.duplicate_count;
            json!({
                "status": "ok",
                "attempted": true,
                "write_mode": write_mode(perception_objects),
                "objects_seen": perception_objects.len(),
                "objects_attempted": response.record_count,
                "objects_written": inserted,
                "objects_deduplicated": deduplicated,
                "objects_failed": 0,
                "elapsed_ms": started.elapsed().as_millis() as u64,
                "database_url": redact_database_url(&database.url),
                "database_url_source": database.source,
                "writer": "write_objects",
                "failure_posture": database_failure_posture(options),
            })
        }
        Err(error) => json!({
            "status": "error",
            "attempted": true,
            "write_mode": write_mode(perception_objects),
            "objects_seen": perception_objects.len(),
            "objects_written": 0,
            "objects_failed": request.records.len(),
            "elapsed_ms": started.elapsed().as_millis() as u64,
            "database_url": redact_database_url(&database.url),
            "database_url_source": database.source,
            "writer": "write_objects",
            "error": sanitize_database_error(&error.to_string(), &database.url),
            "failure_posture": database_failure_posture(options),
        }),
    }
}

fn write_mode(
    perception_objects: &[onecontext_memory_db::write_objects::PerceptionObjectInput],
) -> &'static str {
    let _ = perception_objects;
    "perception_objects"
}

fn ensure_durable_before_cursor_advance(
    db_write: &Value,
    object_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if object_count == 0 || db_write_status(db_write) == Some("ok") {
        return Ok(());
    }
    Err(format!(
        "refusing to advance memoryd cursors: {object_count} perception objects were not written to the database (db_write_status={})",
        db_write_status(db_write).unwrap_or("unknown")
    )
    .into())
}

fn db_write_status(db_write: &Value) -> Option<&str> {
    db_write.get("status").and_then(Value::as_str)
}

fn database_payload(options: &DaemonOptions) -> Value {
    match resolve_storage_backend(options.database.as_ref()) {
        StorageBackendResolution::Backend(StorageBackend::ManagedPostgres) => json!({
            "configured": true,
            "url": null,
            "url_source": "managed_postgres",
            "writer": "write_objects",
        }),
        StorageBackendResolution::Backend(StorageBackend::ExternalPostgres) => {
            match &options.database {
                Some(database) => json!({
                    "configured": true,
                    "url": redact_database_url(&database.url),
                    "url_source": database.source,
                    "writer": "write_objects",
                }),
                None => json!({
                    "configured": false,
                    "url": null,
                    "url_source": null,
                    "writer": "disabled",
                }),
            }
        }
        StorageBackendResolution::Backend(StorageBackend::Disabled)
        | StorageBackendResolution::Invalid(_) => json!({
            "configured": false,
            "url": null,
            "url_source": null,
            "writer": "disabled",
        }),
    }
}

fn storage_posture_payload(options: &DaemonOptions) -> Value {
    let _ = options;
    json!({
        "normal_durable_sink": "database",
        "database_failure_posture": database_failure_posture(options),
        "cursor_advance_rule": "advance after database write succeeds; no fallback cursor advance",
    })
}

fn database_failure_posture(options: &DaemonOptions) -> &'static str {
    let _ = options;
    "fatal_without_database_write"
}

#[cfg(test)]
fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_url_status_redacts_passwords() {
        assert_eq!(
            redact_database_url("postgres://user:secret@localhost:5432/onecontext"),
            "postgres://user:***@localhost:5432/onecontext"
        );
        assert_eq!(
            redact_database_url("host=localhost user=onecontext password=secret dbname=memory"),
            "host=localhost user=onecontext password=*** dbname=memory"
        );
    }

    #[test]
    fn database_success_allows_cursor_advance() {
        let db_write = json!({"status":"ok"});

        ensure_durable_before_cursor_advance(&db_write, 1)
            .expect("database write is the normal durable sink");
    }

    #[test]
    fn database_failure_blocks_cursor_advance_for_objects() {
        let db_write = json!({"status":"error"});

        let result = ensure_durable_before_cursor_advance(&db_write, 2);

        assert!(result.is_err());
    }

    #[test]
    fn no_objects_allow_cursor_save_without_database_write() {
        let db_write = json!({"status":"error"});

        ensure_durable_before_cursor_advance(&db_write, 0)
            .expect("empty ticks are already durable");
    }

    #[test]
    fn batch_summary_reports_objects() {
        let batch = IncrementalIngestBatch {
            connector_key: "codex.local_sessions".to_string(),
            source: "codex".to_string(),
            report: onecontext_memory_db::IncrementalIngestReport {
                connector_key: "codex.local_sessions".to_string(),
                files_seen: 1,
                files_with_new_bytes: 1,
                sqlite_rows_scanned: 0,
                lines_scanned: 2,
                bytes_read: 128,
                objects_emitted: 2,
                reached_event_limit: false,
                reached_line_limit: false,
                partial_line_deferred: false,
            },
            perception_objects: vec![
                test_perception_object("agent/codex/session", "agent_session"),
                test_perception_object("agent/codex/message/1", "agent_message"),
            ],
        };

        let summary = batch_summary("codex", 7, &batch);

        assert_eq!(summary["objects_emitted"], json!(2));
        assert_eq!(summary["object_kinds"]["agent_session"], json!(1));
        assert_eq!(summary["object_kinds"]["agent_message"], json!(1));
    }

    #[test]
    fn database_disabled_summary_counts_objects() {
        let _lock = super::test_env_lock();
        std::env::set_var(STORAGE_BACKEND_ENV, "disabled");
        let options = test_options("db-disabled-summary");
        let objects = vec![test_perception_object(
            "agent/codex/session",
            "agent_session",
        )];

        let summary = write_database_batch(&options, &objects, None);

        assert_eq!(summary["status"], json!("disabled"));
        assert_eq!(summary["write_mode"], json!("perception_objects"));
        assert_eq!(summary["objects_seen"], json!(1));
        assert_eq!(summary["objects_written"], json!(0));
        remove_test_root(&options);
        std::env::remove_var(STORAGE_BACKEND_ENV);
    }

    fn test_options(name: &str) -> DaemonOptions {
        let root =
            std::env::temp_dir().join(format!("onecontext-memoryd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        DaemonOptions {
            home: root.join("home"),
            context_engine_root: root.join("context-engine"),
            run_dir: root.join("run"),
            database: None,
            interval_ms: DEFAULT_INTERVAL_MS,
            max_events: DEFAULT_MAX_EVENTS,
            max_lines: DEFAULT_MAX_LINES,
            sources: vec!["codex".to_string()],
            session_profile: SessionIngestProfile::default(),
            include_sensitive_text: false,
            once: true,
        }
    }

    fn remove_test_root(options: &DaemonOptions) {
        let root = options
            .context_engine_root
            .parent()
            .expect("test root")
            .to_path_buf();
        let _ = fs::remove_dir_all(root);
    }

    fn test_perception_object(
        source_record_key: &str,
        kind: &str,
    ) -> onecontext_memory_db::write_objects::PerceptionObjectInput {
        serde_json::from_value(json!({
            "source_id": "10000000-0000-0000-0000-000000000001",
            "source_record_key": source_record_key,
            "lane_id": "20000000-0000-0000-0000-000000000103",
            "kind": kind,
            "role": "participant",
            "privacy_class": "normal",
            "event_start": "2026-01-01T00:00:00.000000Z",
            "event_end": "2026-01-01T00:00:00.000001Z"
        }))
        .expect("valid perception object")
    }
}

fn load_cursors(path: &PathBuf) -> Result<LocalIngestCursors, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(LocalIngestCursors::default());
    }
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(LocalIngestCursors::default());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn save_cursors(
    path: &PathBuf,
    cursors: &LocalIngestCursors,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(cursors)?)?;
    Ok(())
}

fn eof_cursors_for_source(options: &DaemonOptions, source: &str) -> LocalIngestCursors {
    let mut cursors = LocalIngestCursors::default();
    match source {
        "codex" => seed_file_eof_cursors(
            &options.home.join(".codex/sessions"),
            &mut cursors,
            |path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            },
        ),
        "claude" => seed_file_eof_cursors(
            &options.home.join(".claude/projects"),
            &mut cursors,
            |path| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension == "jsonl")
            },
        ),
        "imessage" => {
            let db_path = options.home.join("Library/Messages/chat.db");
            if let Some(rowid) = max_imessage_rowid(&db_path) {
                cursors.sqlite.insert(
                    db_path.to_string_lossy().to_string(),
                    SqliteIngestCursor {
                        last_rowid: rowid,
                        last_source_date: None,
                    },
                );
            }
        }
        _ => {}
    }
    cursors
}

fn seed_file_eof_cursors(
    root: &PathBuf,
    cursors: &mut LocalIngestCursors,
    predicate: fn(&std::path::Path) -> bool,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            seed_file_eof_cursors(&path, cursors, predicate);
        } else if file_type.is_file() && predicate(&path) {
            if let Ok(metadata) = fs::metadata(&path) {
                cursors.files.insert(
                    path.to_string_lossy().to_string(),
                    FileIngestCursor {
                        offset: metadata.len(),
                        size: metadata.len(),
                        mtime_unix_ns: None,
                        parser_state: Default::default(),
                    },
                );
            }
        }
    }
}

fn max_imessage_rowid(db_path: &PathBuf) -> Option<i64> {
    let connection = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .ok()?;
    connection
        .query_row("SELECT max(ROWID) FROM message", [], |row| row.get(0))
        .ok()
}

fn parse_session_profile(value: &str) -> Result<SessionIngestProfile, Box<dyn std::error::Error>> {
    match value {
        "hot_memory" | "hot-memory" => Ok(SessionIngestProfile::HotMemory),
        "compact_audit" | "compact-audit" => Ok(SessionIngestProfile::CompactAudit),
        "forensic" => Ok(SessionIngestProfile::Forensic),
        other => Err(format!(
            "unknown --profile {other:?}; expected hot_memory, compact_audit, or forensic"
        )
        .into()),
    }
}

fn parse_query_density_request(
    args: &mut Vec<String>,
) -> Result<query_density::QueryDensityRequest, Box<dyn std::error::Error>> {
    let user_id = take_option_value(args, "--user-id").unwrap_or_else(|| "local".to_string());
    let start_time =
        take_option_value(args, "--start-time").or_else(|| take_option_value(args, "--start"));
    let end_time =
        take_option_value(args, "--end-time").or_else(|| take_option_value(args, "--end"));
    let bucket = take_option_value(args, "--bucket").unwrap_or_else(|| "1m".to_string());
    let sources = take_option_value(args, "--sources")
        .or_else(|| take_option_value(args, "--source"))
        .map(|value| split_csv(&value))
        .unwrap_or_default();
    let time = match (start_time, end_time) {
        (Some(start), Some(end)) => Some(read_viewport::TimeRangeRequest { start, end }),
        (None, None) => None,
        _ => return Err("queryDensity requires both --start and --end when either is set".into()),
    };
    ensure_no_args(args)?;
    Ok(query_density::QueryDensityRequest {
        user_id,
        time,
        bucket,
        filters: read_viewport::ObjectFiltersRequest {
            source_types: sources,
            ..read_viewport::ObjectFiltersRequest::default()
        },
        explain: false,
    })
}

fn parse_hydrate_objects_request(
    args: &mut Vec<String>,
) -> Result<hydrate::HydrateObjectsRequest, Box<dyn std::error::Error>> {
    let user_id = take_option_value(args, "--user-id").unwrap_or_else(|| "local".to_string());
    let mut object_ids = Vec::new();
    while let Some(object_id) = take_option_value(args, "--object-id") {
        object_ids.push(object_id);
    }
    if let Some(value) = take_option_value(args, "--object-ids") {
        object_ids.extend(split_csv(&value));
    }
    ensure_no_args(args)?;
    Ok(hydrate::HydrateObjectsRequest {
        user_id,
        object_ids,
        include: hydrate::HydrateIncludeRequest::default(),
    })
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn ensure_no_args(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected arguments: {}", args.join(" ")).into())
    }
}

fn resolve_database_target(args: &mut Vec<String>) -> Option<DatabaseTarget> {
    if let Some(url) = take_option_value(args, "--database-url") {
        return Some(DatabaseTarget {
            url,
            source: "arg:--database-url".to_string(),
        });
    }
    if let Ok(url) = std::env::var(DATABASE_URL_ENV) {
        if !url.trim().is_empty() {
            return Some(DatabaseTarget {
                url,
                source: format!("env:{DATABASE_URL_ENV}"),
            });
        }
    }
    None
}

fn redact_database_url(url: &str) -> String {
    if url.contains("password=") {
        return url
            .split_whitespace()
            .map(|part| {
                if part.starts_with("password=") {
                    "password=***".to_string()
                } else {
                    part.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    let Some(scheme_index) = url.find("://") else {
        return url.to_string();
    };
    let authority_start = scheme_index + 3;
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map(|index| authority_start + index)
        .unwrap_or(url.len());
    let authority = &url[authority_start..authority_end];
    let Some(at_index) = authority.rfind('@') else {
        return url.to_string();
    };
    let userinfo = &authority[..at_index];
    let Some(password_index) = userinfo.rfind(':') else {
        return url.to_string();
    };
    format!(
        "{}{}:{}@{}{}",
        &url[..authority_start],
        &userinfo[..password_index],
        "***",
        &authority[at_index + 1..],
        &url[authority_end..]
    )
}

fn sanitize_database_error(error: &str, database_url: &str) -> String {
    error.replace(database_url, &redact_database_url(database_url))
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn parse_rfc3339_utc(
    field: &str,
    value: &str,
) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .map_err(|error| format!("invalid {field} timestamp {value:?}: {error}"))?
        .with_timezone(&Utc))
}

fn take_option_value(args: &mut Vec<String>, name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    args.remove(index);
    if index >= args.len() {
        return None;
    }
    Some(args.remove(index))
}

fn take_option_value_required(
    args: &mut Vec<String>,
    name: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let Some(index) = args.iter().position(|arg| arg == name) else {
        return Ok(None);
    };
    args.remove(index);
    if index >= args.len() {
        return Err(format!("{name} requires a value").into());
    }
    let value = args.remove(index);
    if value.starts_with("--") {
        return Err(format!("{name} requires a value, got option {value:?}").into());
    }
    Ok(Some(value))
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn print_usage() {
    eprintln!(
        "usage:\n  onecontext-memoryd daemon [--home PATH] [--context-engine-root PATH] [--run-dir PATH] [--database-url URL] [--sources codex,claude,imessage] [--interval-ms N] [--max-events N] [--max-lines N] [--profile hot_memory|compact_audit|forensic] [--once]\n  onecontext-memoryd bench [--home PATH] [--database-url URL] [--sources codex,claude,imessage] [--max-events N] [--max-lines N] [--profile hot_memory|compact_audit|forensic]\n  onecontext-memoryd status [--run-dir PATH] [--context-engine-root PATH]\n  onecontext-memoryd describe\n  onecontext-memoryd protocol|rpc memory.storageHealth --request-json -\n  onecontext-memoryd queryViewport [--database-url URL] [--user-id UUID] [--start RFC3339] [--end RFC3339] [--source KEY] [--limit N]\n  onecontext-memoryd queryDensity [--start-time RFC3339] [--end-time RFC3339] [--sources codex,claude,imessage] [--bucket 1m]\n  onecontext-memoryd hydrateObjects [--object-id ID ... | --object-ids ID,ID]\n\ndatabase URL env: ONECONTEXT_MEMORY_DB_URL\nstorage backend env: ONECONTEXT_STORAGE_BACKEND=managed_postgres|external_postgres|disabled"
    );
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use onecontext_memory_db::ProtocolResponseStatus;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        super::test_env_lock()
    }

    #[test]
    fn protocol_search_semantic_not_ready_uses_error_envelope() {
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: Some("req-semantic".to_string()),
                method: MethodName::SearchSemantic,
                params: json!({
                    "user_id": "00000000-0000-0000-0000-000000000001",
                    "query": "perception db",
                }),
            },
            None,
            7,
        );

        assert_eq!(response.status, ProtocolResponseStatus::Error);
        assert!(response.result.is_none());
        let error = response.error.expect("semantic error");
        assert_eq!(error.code, "SEMANTIC_INDEX_NOT_READY");
        assert!(error.retryable);
        assert_eq!(response.request_id.as_deref(), Some("req-semantic"));
    }

    #[test]
    fn protocol_subscribe_stub_uses_error_envelope() {
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: None,
                method: MethodName::Subscribe,
                params: json!({}),
            },
            None,
            3,
        );

        assert_eq!(response.status, ProtocolResponseStatus::Error);
        assert!(response.result.is_none());
        let error = response.error.expect("subscribe error");
        assert_eq!(error.code, "METHOD_NOT_IMPLEMENTED");
        assert!(!error.retryable);
    }

    #[test]
    fn protocol_query_viewport_reports_storage_not_ready_when_managed_bundle_is_missing() {
        let _lock = env_lock();
        std::env::remove_var(STORAGE_BACKEND_ENV);
        std::env::remove_var(onecontext_memory_db::MANAGED_PG_PREFIX_ENV);
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: None,
                method: MethodName::QueryViewport,
                params: json!({}),
            },
            None,
            1,
        );

        let error = response.error.expect("database error");
        assert_eq!(error.code, "STORAGE_NOT_READY");
        assert!(error.retryable);
    }

    #[test]
    fn protocol_storage_health_defaults_to_managed_scaffold() {
        let _lock = env_lock();
        std::env::remove_var(STORAGE_BACKEND_ENV);
        std::env::remove_var(onecontext_memory_db::MANAGED_PG_PREFIX_ENV);
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: Some("req-storage".to_string()),
                method: MethodName::StorageHealth,
                params: json!({}),
            },
            None,
            2,
        );

        assert_eq!(response.status, ProtocolResponseStatus::Ok);
        let result = response.result.expect("storage result");
        assert_eq!(result["backend"], "managed_postgres");
        assert_eq!(result["status"], "bundle_missing");
        assert_eq!(result["storage_ready"], false);
        assert_eq!(result["recent_backfill"]["status"], "not_checked");
    }

    #[test]
    fn protocol_ensure_storage_ready_accepts_reason() {
        let _lock = env_lock();
        std::env::remove_var(STORAGE_BACKEND_ENV);
        std::env::remove_var(onecontext_memory_db::MANAGED_PG_PREFIX_ENV);
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: Some("req-ensure".to_string()),
                method: MethodName::EnsureStorageReady,
                params: json!({"reason": "test"}),
            },
            None,
            2,
        );

        assert_eq!(response.status, ProtocolResponseStatus::Ok);
        let result = response.result.expect("storage result");
        assert_eq!(result["backend"], "managed_postgres");
        assert_eq!(result["status"], "bundle_missing");
        assert!(result["message"]
            .as_str()
            .is_some_and(|message| message.contains("bundle")));
    }

    #[test]
    fn protocol_storage_disabled_uses_health_envelope() {
        let _lock = env_lock();
        std::env::set_var(STORAGE_BACKEND_ENV, "disabled");
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: None,
                method: MethodName::StorageHealth,
                params: json!({}),
            },
            None,
            2,
        );
        std::env::remove_var(STORAGE_BACKEND_ENV);

        assert_eq!(response.status, ProtocolResponseStatus::Ok);
        let result = response.result.expect("storage result");
        assert_eq!(result["backend"], "disabled");
        assert_eq!(result["status"], "disabled");
    }

    #[test]
    fn protocol_ensure_recent_backfill_returns_pollable_health() {
        let _lock = env_lock();
        std::env::remove_var(STORAGE_BACKEND_ENV);
        std::env::remove_var(onecontext_memory_db::MANAGED_PG_PREFIX_ENV);
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: Some("req-backfill".to_string()),
                method: MethodName::EnsureRecentBackfill,
                params: json!({"reason": "test", "window_hours": 24}),
            },
            None,
            2,
        );

        assert_eq!(response.status, ProtocolResponseStatus::Error);
        let error = response.error.expect("backfill error");
        assert_eq!(error.code, "STORAGE_NOT_READY");
        assert!(error.retryable);
    }

    #[test]
    fn recent_backfill_health_marks_ready_from_density_snapshot() {
        let ingest = onecontext_memory_db::ingest_sources::IngestSourcesResponse {
            ok: true,
            write_id: generated_protocol_write_id(),
            source_results: vec![],
        };
        let health = recent_backfill_health_from_snapshot(
            &onecontext_memory_db::EnsureRecentBackfillRequest {
                reason: "test".to_string(),
                window_hours: 24,
                block_until_ready: false,
                min_density_buckets: Some(2),
            },
            &ingest,
            RecentBackfillSnapshot {
                event_count: 12,
                session_count: 3,
                bucket_count: 2,
                last_successful_ingest_ts: Some(1_780_620_000_000),
            },
        );

        assert_eq!(health.status, "ready");
        assert!(health.density_ready);
        assert_eq!(health.event_count, Some(12));
        assert_eq!(health.session_count, Some(3));
    }

    #[test]
    fn preload_status_stays_unknown_when_setting_is_not_readable() {
        assert_eq!(preload_active_for(true, "timescaledb", None), None);
        assert_eq!(
            preload_active_for(true, "timescaledb", Some("timescaledb,pg_stat_statements")),
            Some(true)
        );
        assert_eq!(
            preload_active_for(true, "timescaledb", Some("pg_stat_statements")),
            Some(false)
        );
        assert_eq!(
            preload_active_for(false, "pg_trgm", Some("timescaledb")),
            None
        );
    }

    #[test]
    fn managed_readable_sources_only_keeps_present_roots() {
        let root = std::env::temp_dir().join(format!(
            "onecontext-memoryd-readable-sources-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".codex/sessions")).expect("codex root");
        fs::create_dir_all(root.join(".claude/projects")).expect("claude root");

        let sources = managed_readable_sources(root.clone());

        assert_eq!(sources, vec!["codex".to_string(), "claude".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protocol_storage_diagnostics_returns_typed_scaffold() {
        let _lock = env_lock();
        std::env::set_var(STORAGE_BACKEND_ENV, "disabled");
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: Some("req-diagnostics".to_string()),
                method: MethodName::StorageDiagnostics,
                params: json!({"include_logs": true}),
            },
            None,
            2,
        );
        std::env::remove_var(STORAGE_BACKEND_ENV);

        assert_eq!(response.status, ProtocolResponseStatus::Ok);
        let result = response.result.expect("diagnostics result");
        assert_eq!(result["ok"], true);
        assert_eq!(result["status"], "disabled");
    }

    #[test]
    fn protocol_repair_storage_returns_health_scaffold() {
        let _lock = env_lock();
        std::env::remove_var(STORAGE_BACKEND_ENV);
        std::env::remove_var(onecontext_memory_db::MANAGED_PG_PREFIX_ENV);
        let response = dispatch_protocol_request(
            ProtocolRequest {
                schema_version: 1,
                request_id: Some("req-repair".to_string()),
                method: MethodName::RepairStorage,
                params: json!({"reason": "test"}),
            },
            None,
            2,
        );

        assert_eq!(response.status, ProtocolResponseStatus::Ok);
        let result = response.result.expect("repair result");
        assert_eq!(result["backend"], "managed_postgres");
        assert_eq!(result["status"], "bundle_missing");
    }

    #[test]
    fn protocol_write_error_codes_are_preserved() {
        let error = onecontext_memory_db::write_objects::WriteObjectsError::InvalidRecord {
            field: "kind",
            value: "".to_string(),
            reason: "kind must not be empty".to_string(),
        };

        let (code, retryable) = protocol_error_code_and_retryability(&error, "WRITE_FAILED");

        assert_eq!(code, "INVALID_RECORD");
        assert!(!retryable);
    }

    #[test]
    fn protocol_request_json_requires_a_value() {
        let mut args = vec!["memory.describe".to_string(), "--request-json".to_string()];

        let error = run_protocol(&mut args).expect_err("missing request path should fail");

        assert!(error
            .to_string()
            .contains("--request-json requires a value"));
    }
}
