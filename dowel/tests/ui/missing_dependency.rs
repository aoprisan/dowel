// The repair signal: a field whose type has no `Wire<Ctx>` impl must fail to
// compile with `the trait bound `Db: Wire<AppCtx>` is not satisfied`.
use dowel::Wire;

struct AppCtx;

// Note: NO `impl Wire<AppCtx> for Db`.
struct Db {
    #[allow(dead_code)]
    url: &'static str,
}

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}

fn main() {
    let ctx = AppCtx;
    let _repo = PlayerRepo::wire(&ctx);
}
