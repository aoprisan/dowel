// Happy path: a leaf taught to the context, wired up a two-level graph,
// covering generics along the way.
use dowel::Wire;

struct AppCtx {
    db: Db,
}

#[derive(Clone)]
struct Db {
    url: &'static str,
}
impl Wire<AppCtx> for Db {
    fn wire(ctx: &AppCtx) -> Self {
        ctx.db.clone()
    }
}

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}

#[derive(Wire)]
struct PlayerService {
    repo: PlayerRepo,
}

// A generic service: its own generics and bounds are preserved.
#[derive(Wire)]
struct Wrapper<T: Clone> {
    inner: T,
}

fn main() {
    let ctx = AppCtx {
        db: Db { url: "pg://" },
    };
    let svc = PlayerService::wire(&ctx);
    assert_eq!(svc.repo.db.url, "pg://");

    let w: Wrapper<Db> = Wrapper::wire(&ctx);
    assert_eq!(w.inner.url, "pg://");
}
