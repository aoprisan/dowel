//! The whole stack in one file: **axum + dowel + CQRS + an actor**.
//!
//! This is the end-to-end shape the other examples show in isolation. Read it
//! top-to-bottom as one request's life:
//!
//! ```text
//! HTTP
//!  -> axum router
//!    -> Wired<PlayerService>          // dowel wires the graph from State, zero-cost
//!      -> svc.handle(CreatePlayer)    // CQRS write: static dispatch, a plain call
//!        -> repo.insert(..)           // wired service over a concrete Db leaf
//!        -> ledger.credit(..).await   // actor leaf: the ONE dynamic hop
//! ```
//!
//! The four layers map onto four *distinct* concerns, and that is why they
//! compose without fighting:
//!   - **axum** owns the network edge; its `State` *is* the composition root.
//!   - **dowel** owns wiring; everything is either a leaf or a `#[derive(Wire)]`
//!     service. There is no second wiring philosophy in the file.
//!   - **CQRS** owns application logic. Writes go through `Handles<C>` (static,
//!     monomorphized — rule 6, no command bus). Reads do NOT: a query handler is
//!     just `Wired<ReadRepo>` straight to a read pool, off the command path.
//!   - **actors** own single-owner mutable state. The actor is a `mpsc::Sender`
//!     leaf (rule 5). Its mailbox is the only dynamic dispatch seam in the graph.
//!
//! Run: `DOWEL_SERVE=1 cargo run --example stack`, then
//! `curl -X POST localhost:3000/player/ada` and `curl localhost:3000/player/ada`.

use std::collections::HashMap;
use std::convert::Infallible;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use dowel::{Context, Wire};
use tokio::sync::{mpsc, oneshot};

// ===========================================================================
// LEAVES — the only place real infrastructure lives. All `Clone`, all cheap
// (rule 4): `Arc`-backed handles or `Copy`. `#[derive(Context)]` teaches the
// root context to clone each one out, so we never hand-write the leaf impls.
//
// Writes and reads get *separate* handles (`Db` vs `ReadDb`) on purpose: that
// is the C/Q split expressed as wiring, not as a runtime branch.
// ===========================================================================

#[derive(Clone)]
struct Db {
    url: &'static str,
}

#[derive(Clone)]
struct ReadDb {
    url: &'static str,
}

#[derive(Clone, Copy)]
struct Clock {
    epoch: u64,
}

/// The actor's address: a named concrete handle around the raw sender (rule 1 —
/// never `Arc<dyn>`). Cloning it is cloning the `mpsc::Sender`.
#[derive(Clone)]
struct Ledger(mpsc::Sender<LedgerMsg>);

impl Ledger {
    /// The async edge. Send + await the reply — this lives in a method, never in
    /// `wire` (rule 4 keeps wiring pure and `'static`-clonable).
    async fn credit(&self, player: &str, amount: u64) -> u64 {
        let (reply, rx) = oneshot::channel();
        self.0
            .send(LedgerMsg::Credit {
                player: player.to_owned(),
                amount,
                reply,
            })
            .await
            .expect("ledger task alive");
        rx.await.expect("ledger task replied")
    }
}

/// The ONE concrete context. `Clone` for axum's `State`; `Context` to derive a
/// `Wire<AppCtx>` leaf impl per field. Four distinct field types => no collision.
#[derive(Clone, Context)]
struct AppCtx {
    db: Db,
    read_db: ReadDb,
    clock: Clock,
    ledger: Ledger,
}

// ===========================================================================
// THE ACTOR — single owner of mutable balance state. The graph never sees the
// `HashMap`; it only ever wires the `Ledger` handle. This is the exception, not
// the architecture: reach for it only when state genuinely has one owner.
// ===========================================================================

enum LedgerMsg {
    Credit {
        player: String,
        amount: u64,
        reply: oneshot::Sender<u64>,
    },
}

/// The actor's *initial state* is wired like anything else: `clock` is a leaf
/// dependency; `balances` has no `Wire` impl, so `#[wire(skip)]` defaults it.
#[derive(Wire)]
struct LedgerState {
    clock: Clock,
    #[wire(skip)]
    balances: HashMap<String, u64>,
}

async fn ledger_task(mut state: LedgerState, mut rx: mpsc::Receiver<LedgerMsg>) {
    println!("ledger actor started at epoch {}", state.clock.epoch);
    while let Some(msg) = rx.recv().await {
        match msg {
            LedgerMsg::Credit {
                player,
                amount,
                reply,
            } => {
                let bal = state.balances.entry(player).or_insert(0);
                *bal += amount;
                let _ = reply.send(*bal);
            }
        }
    }
}

// ===========================================================================
// WRITE SIDE (the "C") — a wired service dispatched through `Handles<C>`.
// Dependencies are concrete fields (rule 1); adding the actor was the same edit
// as adding the repo.
// ===========================================================================

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}
impl PlayerRepo {
    async fn insert(&self, name: &str) -> u64 {
        let _ = (self.db.url, name); // pretend to hit the write pool
        7 // freshly minted id
    }
}

#[derive(Wire)]
struct PlayerService {
    repo: PlayerRepo,
    ledger: Ledger,
}

/// CQRS dispatch. `handle` is static and monomorphized — it compiles to a plain
/// method call, not a lookup. No bus, no `dyn`, no registry (rule 6). Native
/// `async fn` in the trait is fine precisely because we always call it by the
/// concrete `PlayerService` type and never need object safety.
trait Handles<C> {
    type Output;
    async fn handle(&self, cmd: C) -> Self::Output;
}

struct CreatePlayer {
    name: String,
}

/// One command, handled directly: persist the player, then credit a signup
/// bonus through the actor. Two collaborators, one static call site.
impl Handles<CreatePlayer> for PlayerService {
    type Output = (u64, u64); // (player id, starting balance)
    async fn handle(&self, cmd: CreatePlayer) -> (u64, u64) {
        let id = self.repo.insert(&cmd.name).await;
        let balance = self.ledger.credit(&cmd.name, 100).await;
        (id, balance)
    }
}

// ===========================================================================
// READ SIDE (the "Q") — deliberately NOT a command. A query handler is just a
// wired service over the read pool, called directly from the axum handler. It
// never touches `Handles`, the actor, or the write `Db`. This keeps the hot read
// path a straight monomorphized call.
// ===========================================================================

#[derive(Wire)]
struct PlayerReadRepo {
    read_db: ReadDb,
    clock: Clock,
}
impl PlayerReadRepo {
    async fn balance_of(&self, name: &str) -> String {
        let _ = (self.read_db.url, name); // pretend to hit the read replica
        format!("read player {name} as of epoch {}", self.clock.epoch)
    }
}

// ===========================================================================
// THE EDGE — `Wired<S>` extractor: `S::wire(state)` straight from axum State.
// `Wired` is our own newtype, so the blanket has no coherence clash with axum's
// own `FromRequestParts` blankets. (Kept an example, not facade code — see the
// CLAUDE.md "settled decisions".)
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

// The dependency is declared in each handler's signature. A write dispatches a
// command; a read calls its repo directly. Same `Wired<X>` mechanism, different
// service — the difference between C and Q is which service you ask for.

async fn create_player(Wired(svc): Wired<PlayerService>, Path(name): Path<String>) -> impl IntoResponse {
    let (id, balance) = svc.handle(CreatePlayer { name }).await;
    format!("created player {id} with balance {balance}")
}

async fn get_player(Wired(repo): Wired<PlayerReadRepo>, Path(name): Path<String>) -> impl IntoResponse {
    repo.balance_of(&name).await
}

#[tokio::main]
async fn main() {
    // Composition root: the sender exists before the actor is spawned, so the
    // ONE context can be built first and used to wire both the actor's state and
    // every per-request service.
    let (tx, rx) = mpsc::channel(32);
    let ctx = AppCtx {
        db: Db { url: "pg://write" },
        read_db: ReadDb { url: "pg://read-replica" },
        clock: Clock { epoch: 100 },
        ledger: Ledger(tx),
    };

    // Wire the actor's initial state from the context, then hand it to the runtime.
    tokio::spawn(ledger_task(LedgerState::wire(&ctx), rx));

    let app: Router = Router::new()
        .route("/player/{name}", post(create_player))
        .route("/player/{name}", get(get_player))
        .with_state(ctx);

    if std::env::var_os("DOWEL_SERVE").is_some() {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    } else {
        let _ = app;
        println!("stack example wired (axum + dowel + cqrs + actor); set DOWEL_SERVE=1 to serve");
    }
}
