//! `#[derive(Context)]` generates the leaf `Wire` impls for each field, so a
//! service wires straight from the context with no hand-written leaf impls.
//! `#[context(skip)]` keeps a non-leaf field out of the wiring.

use hewn::{Context, Wire};

#[derive(Clone)]
struct Db {
    url: &'static str,
}

#[derive(Clone, Copy)]
struct Clock;

#[derive(Context)]
struct AppCtx {
    db: Db,
    clock: Clock,
    #[context(skip)]
    name: &'static str,
}

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}

#[derive(Wire)]
struct PlayerService {
    repo: PlayerRepo,
    clock: Clock,
}

fn main() {
    let ctx = AppCtx {
        db: Db { url: "pg://" },
        clock: Clock,
        name: "ignored",
    };
    let svc = PlayerService::wire(&ctx);
    assert_eq!(svc.repo.db.url, "pg://");
    let _ = svc.clock;
    let _ = ctx.name;
}
