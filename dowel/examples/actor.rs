//! Actors as a dowel *leaf* — hand-rolled tokio `mpsc`, zero framework deps.
//!
//! This is the most idiomatic fit, because the handle the graph hands around is
//! exactly what CLAUDE.md rule 5 already names:
//!
//! > Singletons live in the leaf, not the graph. If one shared instance is
//! > required, the leaf is `Arc<Mutex<_>>` or an `mpsc` sender to a single
//! > owning task.
//!
//! An `mpsc::Sender` to one owning task *is* a hand-rolled actor. So the seam
//! between dowel and the actor is: the running actor is spawned at the
//! composition root; its sender is a `Clone` leaf taught to the context. Wiring
//! `wire()` only ever *clones the handle* — it stays pure construction (no
//! async, rule 4 keeps it `'static`-clonable). Sending a message is async and
//! lives in a service method, never in `wire`.
//!
//! The mailbox is a *dynamic* dispatch seam (messages queue, replies come back
//! over a `oneshot`). Rule 6 allows exactly this as an explicit isolated seam
//! for one named reason — here, single-owner mutable state — rather than as the
//! default call path.
//!
//! Run: `cargo run --example actor`

use std::collections::HashMap;

use dowel::{Context, Wire};
use tokio::sync::{mpsc, oneshot};

// ---------------------------------------------------------------------------
// Leaves. `#[derive(Context)]` teaches the root context to clone each one out.
//
// `Ledger` wraps the raw `mpsc::Sender` in a named concrete handle (rule 1: a
// dependency is a concrete type, never `Arc<dyn>`). It is the actor's address.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Clock {
    epoch: u64,
}

#[derive(Clone)]
struct Ledger(mpsc::Sender<LedgerMsg>);

impl Ledger {
    /// The async edge: send a message and await the reply. Not in `wire`.
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

#[derive(Context)]
struct AppCtx {
    clock: Clock,
    ledger: Ledger,
}

// ---------------------------------------------------------------------------
// The actor protocol + its owning task. The task is the single owner of the
// mutable state; the graph never sees it, only the `Ledger` handle.
// ---------------------------------------------------------------------------

enum LedgerMsg {
    Credit {
        player: String,
        amount: u64,
        reply: oneshot::Sender<u64>,
    },
}

/// The actor's *initial state* is wired from the context (rule 2 builds it),
/// then the runtime owns spawning it (rule 5 keeps it a singleton). `clock` is a
/// wired leaf dependency; `balances` has no `Wire` impl, so `#[wire(skip)]`
/// defaults it.
#[derive(Wire)]
struct LedgerState {
    clock: Clock,
    #[wire(skip)]
    balances: HashMap<String, u64>,
}

async fn ledger_task(mut state: LedgerState, mut rx: mpsc::Receiver<LedgerMsg>) {
    println!("ledger task started at epoch {}", state.clock.epoch);
    while let Some(msg) = rx.recv().await {
        match msg {
            LedgerMsg::Credit {
                player,
                amount,
                reply,
            } => {
                let balance = state.balances.entry(player).or_insert(0);
                *balance += amount;
                let _ = reply.send(*balance);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// A service depends on the actor by its concrete `Ledger` handle (rule 1) —
// just another wired field. Adding it was the same edit as any dependency.
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct BillingService {
    ledger: Ledger,
}

impl BillingService {
    async fn charge(&self, player: &str, amount: u64) -> u64 {
        self.ledger.credit(player, amount).await
    }
}

#[tokio::main]
async fn main() {
    // The sender exists before the actor is spawned, so the ONE concrete context
    // can be built first and used to wire both the actor's state and the service.
    let (tx, rx) = mpsc::channel(32);
    let ctx = AppCtx {
        clock: Clock { epoch: 100 },
        ledger: Ledger(tx),
    };

    // Wire the actor's state from the context, then hand it to the runtime.
    let state = LedgerState::wire(&ctx);
    tokio::spawn(ledger_task(state, rx));

    // Wire a normal service that talks to the actor.
    let billing = BillingService::wire(&ctx);

    let first = billing.charge("ada", 10).await;
    let second = billing.charge("ada", 5).await;

    assert_eq!(first, 10);
    assert_eq!(second, 15); // same owning task accumulated the balance
    println!("billing via mpsc actor: {first} then {second}");
}
