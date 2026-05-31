//! Actors as a dowel *leaf* — Actix edition.
//!
//! Same shape as `examples/actor.rs`, with a real actor framework instead of a
//! hand-rolled `mpsc` loop. Actix gives you an `Addr<A>` mailbox handle that is
//! `Clone` and `Send + 'static` — which is precisely a dowel leaf (rule 4). We
//! wrap it in a named `Ledger` handle (rule 1: a dependency is a concrete type)
//! and teach the context to clone it out.
//!
//! The split of responsibility is the whole point:
//!   - `#[derive(Wire)]` builds the actor *struct* from its own dependencies
//!     (rule 2 owns construction),
//!   - the Actix runtime owns *starting* it (`.start()` makes the one
//!     singleton — rule 5; the graph never deduplicates, so we spawn once at the
//!     root and pass the address around).
//!
//! `Handler<M>` is Actix's dynamic dispatch seam — the documented exception
//! rule 6 allows for a named reason (single-owner state), not the default path.
//!
//! Run: `cargo run --example actix`

use actix::prelude::*;
use dowel::{Context, Wire};

// ---------------------------------------------------------------------------
// A leaf the actor itself depends on. The actor's deps are wired like any
// other service's (rule 2), so a tiny bootstrap context provides them.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Db {
    url: &'static str,
}

// ---------------------------------------------------------------------------
// The actor. `#[derive(Wire)]` builds its initial state from a context; the
// Actix runtime starts it. `balance` has no `Wire` impl, so it is skipped.
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct LedgerActor {
    db: Db,
    #[wire(skip)]
    balance: u64,
}

impl Actor for LedgerActor {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "u64")]
struct Credit {
    amount: u64,
}

impl Handler<Credit> for LedgerActor {
    type Result = u64;

    fn handle(&mut self, msg: Credit, _ctx: &mut Context<Self>) -> u64 {
        let _ = self.db.url; // pretend the actor persists through `db`
        self.balance += msg.amount;
        self.balance
    }
}

// ---------------------------------------------------------------------------
// The actor's address as a named leaf (rule 1). `Addr<LedgerActor>` is `Clone`,
// so `Ledger` is a cheap clonable handle (rule 4).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Ledger(Addr<LedgerActor>);

impl Ledger {
    /// The async edge: `addr.send(..)` returns a future. Not in `wire`.
    async fn credit(&self, amount: u64) -> u64 {
        self.0.send(Credit { amount }).await.expect("mailbox alive")
    }
}

#[derive(Context)]
struct AppCtx {
    db: Db,
    ledger: Ledger,
}

// ---------------------------------------------------------------------------
// A service depends on the actor by its concrete `Ledger` handle — just another
// wired field.
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct BillingService {
    ledger: Ledger,
}

impl BillingService {
    async fn charge(&self, amount: u64) -> u64 {
        self.ledger.credit(amount).await
    }
}

#[actix::main]
async fn main() {
    let db = Db {
        url: "pg://localhost",
    };

    // 1. Wire the actor's state from the leaves it needs (just `Db`), then let
    //    the runtime own starting it. The actor does not depend on its own
    //    address, so a tiny bootstrap context suffices here.
    struct Boot {
        db: Db,
    }
    impl Wire<Boot> for Db {
        fn wire(c: &Boot) -> Self {
            c.db.clone()
        }
    }
    let actor = LedgerActor::wire(&Boot { db: db.clone() });
    let ledger = Ledger(actor.start());

    // 2. The ONE composition root: the address joins as a leaf so any service
    //    can reach the single running actor.
    let ctx = AppCtx { db, ledger };

    // 3. Wire a normal service that talks to the actor.
    let billing = BillingService::wire(&ctx);

    let first = billing.charge(10).await;
    let second = billing.charge(5).await;

    assert_eq!(first, 10);
    assert_eq!(second, 15); // same actor accumulated the balance
    println!("billing via actix actor: {first} then {second}");
}
