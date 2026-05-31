//! `Wired<S>` — declare a dependency directly in an axum handler signature.
//!
//! The axum `State` *is* the composition root: it holds the one concrete
//! context. `Wired<S>` is an extractor that calls `S::wire(&ctx)` for any
//! service `S: Wire<Ctx>`, so a handler asks for exactly the slice of the graph
//! it needs and nothing more. Adding a dependency to a handler is always the
//! same edit: add a `Wired<X>` parameter.
//!
//! Run: `cargo run --example axum` then `curl localhost:3000/player/7`.

use std::convert::Infallible;

use axum::{
    extract::{FromRef, FromRequestParts, Path},
    http::request::Parts,
    response::IntoResponse,
    routing::get,
    Router,
};
use hewn::Wire;

// ---------------------------------------------------------------------------
// The leaf and the context (the composition root lives in `main`).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Db {
    url: &'static str,
}
impl Wire<AppCtx> for Db {
    fn wire(ctx: &AppCtx) -> Self {
        ctx.db.clone()
    }
}

#[derive(Clone)]
struct AppCtx {
    db: Db,
}

// ---------------------------------------------------------------------------
// Services derive their wiring.
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}
impl PlayerRepo {
    fn find(&self, id: u64) -> String {
        format!("player {id} from {}", self.db.url)
    }
}

// ---------------------------------------------------------------------------
// The `Wired<S>` extractor: `S::wire(&ctx)` from the axum State.
//
// `Ctx: FromRef<AppState>` lets the extractor pull the context out of whatever
// state the app is built with. The blanket impl works for every `S: Wire<Ctx>`
// without per-service glue.
// ---------------------------------------------------------------------------

struct Wired<S>(pub S);

impl<AppState, Ctx, S> FromRequestParts<AppState> for Wired<S>
where
    AppState: Send + Sync,
    Ctx: FromRef<AppState>,
    S: Wire<Ctx>,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let ctx = Ctx::from_ref(state);
        Ok(Wired(S::wire(&ctx)))
    }
}

// ---------------------------------------------------------------------------
// Handler: the dependency is declared in the signature.
// ---------------------------------------------------------------------------

async fn get_player(Wired(repo): Wired<PlayerRepo>, Path(id): Path<u64>) -> impl IntoResponse {
    repo.find(id)
}

#[tokio::main]
async fn main() {
    // The ONE place a concrete context exists.
    let ctx = AppCtx {
        db: Db { url: "pg://localhost" },
    };

    let app: Router = Router::new()
        .route("/player/:id", get(get_player))
        .with_state(ctx);

    // Demonstrate it builds and the extractor type-checks. Serve if run directly.
    if std::env::var_os("HEWN_SERVE").is_some() {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    } else {
        // Touch `app` so the example is a real compile/type check by default.
        let _ = app;
        println!("axum example wired; set HEWN_SERVE=1 to actually serve");
    }
}
