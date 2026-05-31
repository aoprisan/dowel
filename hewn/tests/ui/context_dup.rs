//! Two non-skipped context fields of the same type would emit conflicting
//! `Wire` impls. `#[derive(Context)]` rejects that with a readable message
//! pointing at the duplicate field (skip one and wire it by hand instead).

use hewn::Context;

#[derive(Clone)]
struct Db {
    url: &'static str,
}

#[derive(Context)]
struct AppCtx {
    primary: Db,
    replica: Db,
}

fn main() {}
