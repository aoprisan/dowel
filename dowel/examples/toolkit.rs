//! The stack with **real infrastructure**: a live `sqlx` SQLite pool as the
//! `Db` leaf, a `tracing` audit sink behind an actor, and *two* CQRS commands —
//! so you can see that "add a feature" is just "add a `Handles` impl".
//!
//! This is `stack.rs` taken one step toward a usable toolkit. Nothing about the
//! wiring changed: a real `SqlitePool` is `Arc`-backed and `Clone`, so it drops
//! straight into a leaf (rule 4) exactly where the fake `Db { url }` used to sit.
//! That is the point — swapping a stub for production infrastructure is a leaf
//! edit, not a graph edit.
//!
//! Layers, same as `stack.rs`:
//!   - **sqlx SQLite** — the `Db`/`ReadDb` leaves wrap a real `Pool<Sqlite>`.
//!     Writes and reads get separate handles (the C/Q split as wiring), and they
//!     are *independent connection pools* over one SQLite file — the real shape
//!     of a write primary + read replica, differing from a true replica only by
//!     replication. Swapping `ReadDb` to a replica DSN is a composition-root edit.
//!   - **dowel** — every repo/service is `#[derive(Wire)]`; the leaves are taught
//!     in bulk by `#[derive(Context)]`.
//!   - **CQRS** — `Handles<CreatePlayer>` and `Handles<RenamePlayer>` on the same
//!     service, both static/monomorphized (rule 6). Adding the second command
//!     added one `impl` block and one route — no new wiring decision.
//!   - **actor + tracing** — the `Audit` leaf is an `mpsc::Sender` to a single
//!     owning task that emits `tracing` events. Fire-and-forget (no `oneshot`),
//!     so it shows the reply-less actor flavor and doubles as the tracing sink.
//!
//! Errors are threaded as `Result` end-to-end: repos surface `sqlx::Error`, the
//! `Handles` impls and read repo bubble it into an `AppError`, and `AppError`
//! implements `IntoResponse` so the request path never panics — only the
//! composition root (startup) uses `expect`, where a failure should abort.
//!
//! `main` runs a real scenario against SQLite on every invocation (so the example
//! proves itself), then serves only when asked:
//!   `DOWEL_SERVE=1 cargo run --example toolkit`
//!   `curl -X POST localhost:3000/player/ada`
//!   `curl -X PUT  localhost:3000/player/1/lovelace`
//!   `curl          localhost:3000/players/1`

use std::convert::Infallible;

use axum::{
    extract::{FromRequestParts, Path},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use dowel::{Context, Wire};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

// ===========================================================================
// LEAVES — real handles now. `SqlitePool` is already `Clone` + `Arc`-backed, so
// wrapping it in a named newtype (rule 1) is the whole leaf. `#[derive(Context)]`
// teaches the root to clone each one out.
// ===========================================================================

/// Write pool.
#[derive(Clone)]
struct Db(SqlitePool);

/// Read pool — a distinct handle (the C/Q split, expressed as wiring) and a
/// distinct connection pool. Points at the same SQLite file as `Db` here; at a
/// read replica in production (a DSN change at the composition root, nothing else).
#[derive(Clone)]
struct ReadDb(SqlitePool);

/// Audit sink: the address of the tracing actor (rule 5 — a singleton owning task
/// behind a clonable `mpsc::Sender`).
#[derive(Clone)]
struct Audit(mpsc::Sender<AuditEvent>);

impl Audit {
    /// Fire-and-forget: no `oneshot`, no await on a reply. A full mailbox applies
    /// backpressure; a dead sink is logged, not panicked (it must never break the
    /// request path). This is the async edge — never in `wire`.
    async fn record(&self, event: AuditEvent) {
        if self.0.send(event).await.is_err() {
            tracing::warn!("audit sink down; event dropped");
        }
    }
}

#[derive(Clone, Context)]
struct AppCtx {
    db: Db,
    read_db: ReadDb,
    audit: Audit,
}

// ===========================================================================
// THE TRACING ACTOR — single owner of the audit stream. The graph only ever
// wires the `Audit` handle; the task is spawned once at the root.
// ===========================================================================

enum AuditEvent {
    Created { id: i64, name: String },
    Renamed { id: i64, name: String },
}

async fn audit_task(mut rx: mpsc::Receiver<AuditEvent>) {
    tracing::info!("audit sink started");
    while let Some(event) = rx.recv().await {
        match event {
            AuditEvent::Created { id, name } => tracing::info!(id, %name, "player created"),
            AuditEvent::Renamed { id, name } => tracing::info!(id, %name, "player renamed"),
        }
    }
}

// ===========================================================================
// WRITE SIDE (the "C") — one service, two commands. Both dependencies are
// concrete wired fields (rule 1).
// ===========================================================================

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}
impl PlayerRepo {
    async fn insert(&self, name: &str) -> Result<i64, sqlx::Error> {
        let res = sqlx::query("INSERT INTO players (name) VALUES (?)")
            .bind(name)
            .execute(&self.db.0)
            .await?;
        Ok(res.last_insert_rowid())
    }

    async fn rename(&self, id: i64, name: &str) -> Result<u64, sqlx::Error> {
        let res = sqlx::query("UPDATE players SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&self.db.0)
            .await?;
        Ok(res.rows_affected())
    }
}

#[derive(Wire)]
struct PlayerService {
    repo: PlayerRepo,
    audit: Audit,
}

/// CQRS dispatch — static, monomorphized, no bus (rule 6). Native `async fn` in
/// the trait is fine because we only ever call it by concrete type.
trait Handles<C> {
    type Output;
    async fn handle(&self, cmd: C) -> Self::Output;
}

struct CreatePlayer {
    name: String,
}
struct RenamePlayer {
    id: i64,
    name: String,
}

/// Command #1.
impl Handles<CreatePlayer> for PlayerService {
    type Output = Result<i64, AppError>;
    async fn handle(&self, cmd: CreatePlayer) -> Result<i64, AppError> {
        let id = self.repo.insert(&cmd.name).await?;
        self.audit
            .record(AuditEvent::Created { id, name: cmd.name })
            .await;
        Ok(id)
    }
}

/// Command #2 — adding it was *only* this impl plus one route below. No new
/// wiring, no change to the service's fields, no touch to command #1.
impl Handles<RenamePlayer> for PlayerService {
    type Output = Result<u64, AppError>;
    async fn handle(&self, cmd: RenamePlayer) -> Result<u64, AppError> {
        let affected = self.repo.rename(cmd.id, &cmd.name).await?;
        self.audit
            .record(AuditEvent::Renamed {
                id: cmd.id,
                name: cmd.name,
            })
            .await;
        Ok(affected)
    }
}

// ===========================================================================
// READ SIDE (the "Q") — not a command. A wired service over the read pool,
// called directly from the handler. Never touches `Handles`, the actor, or `Db`.
// ===========================================================================

#[derive(Wire)]
struct PlayerReadRepo {
    read_db: ReadDb,
}
impl PlayerReadRepo {
    async fn find(&self, id: i64) -> Result<Option<(i64, String)>, sqlx::Error> {
        sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM players WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.read_db.0)
            .await
    }
}

// ===========================================================================
// ERRORS — one application error that knows how to become an HTTP response. The
// `?` operator threads `sqlx::Error` up through the repos and `Handles` impls and
// converts it here; `NotFound` is raised by the read handler. This is the seam
// that keeps the request path panic-free.
// ===========================================================================

#[derive(Debug)]
enum AppError {
    Db(sqlx::Error),
    NotFound,
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Db(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            AppError::Db(e) => {
                // Log the detail, return an opaque 500 — never leak the DB error.
                tracing::error!(error = %e, "db error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
        }
    }
}

// ===========================================================================
// THE EDGE — `Wired<S>` extractor + handlers. A write dispatches a command; a
// read calls its repo directly. Same mechanism, different service. Each handler
// returns `Result<_, AppError>`, so an error becomes a response, not a panic.
// ===========================================================================

struct Wired<S>(pub S);

impl<State, S> FromRequestParts<State> for Wired<S>
where
    State: Send + Sync,
    S: Wire<State>,
{
    type Rejection = Infallible;
    async fn from_request_parts(_parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        Ok(Wired(S::wire(state)))
    }
}

async fn create_player(
    Wired(svc): Wired<PlayerService>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id = svc.handle(CreatePlayer { name }).await?;
    Ok(format!("created player {id}"))
}

async fn rename_player(
    Wired(svc): Wired<PlayerService>,
    Path((id, name)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let affected = svc.handle(RenamePlayer { id, name }).await?;
    Ok(format!("renamed {affected} player(s)"))
}

async fn get_player(
    Wired(repo): Wired<PlayerReadRepo>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let (id, name) = repo.find(id).await?.ok_or(AppError::NotFound)?;
    Ok(format!("player {id}: {name}"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    // Composition root — the one place `expect` is right: a failure here should
    // abort startup, not be turned into a response. Two *independent* pools over
    // one SQLite file model a write primary + read replica (WAL lets the reader
    // run concurrently with the writer). A fresh file each run keeps it idempotent.
    let path = std::env::temp_dir().join("dowel-toolkit.sqlite");
    let _ = std::fs::remove_file(&path);
    let connect = |writable: bool| {
        SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(writable)
            .read_only(!writable)
            .journal_mode(SqliteJournalMode::Wal)
    };
    let write_pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(connect(true))
        .await
        .expect("open write pool");
    sqlx::query("CREATE TABLE players (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&write_pool)
        .await
        .expect("create schema");
    let read_pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(connect(false)) // read-only handle to the same file
        .await
        .expect("open read pool");

    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(audit_task(rx));

    let ctx = AppCtx {
        db: Db(write_pool),
        read_db: ReadDb(read_pool),
        audit: Audit(tx),
    };

    // Prove the whole stack end-to-end against real SQLite on every run.
    let writer = PlayerService::wire(&ctx);
    let reader = PlayerReadRepo::wire(&ctx);
    let id = writer
        .handle(CreatePlayer { name: "ada".into() })
        .await
        .expect("create");
    writer
        .handle(RenamePlayer {
            id,
            name: "lovelace".into(),
        })
        .await
        .expect("rename");
    let found = reader.find(id).await.expect("read");
    println!("scenario: {found:?}"); // Some((1, "lovelace"))

    let app: Router = Router::new()
        .route("/player/{name}", post(create_player))
        .route("/player/{id}/{name}", put(rename_player))
        .route("/players/{id}", get(get_player))
        .with_state(ctx);

    if std::env::var_os("DOWEL_SERVE").is_some() {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    } else {
        let _ = app;
        println!("toolkit example wired (sqlx sqlite + tracing actor + cqrs); set DOWEL_SERVE=1 to serve");
    }
}
