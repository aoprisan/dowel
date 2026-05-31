//! `Wired<S>` on **pre-0.8 axum (0.7)** — the `#[async_trait]` form.
//!
//! Same convention as `examples/axum.rs`, kept for consumers still on axum 0.7.
//! Two things differ from the 0.8 examples:
//!   - axum 0.7's `FromRequestParts` is an `#[async_trait]` (see axum-core), so
//!     the `Wired<S>` blanket carries the same attribute. The impl still
//!     compiles cleanly — `Wired` is our own newtype, no coherence clash.
//!   - Path captures use colon syntax: `/player/:id`, not `/player/{id}`.
//!
//! The crate is pulled in renamed (`axum07 = { package = "axum", version =
//! "0.7" }`) so 0.7 and 0.8 can coexist in dev-deps; we alias it back to `axum`
//! here so the body reads as ordinary axum 0.7 code.
//!
//! Run: `HEWN_SERVE=1 cargo run --example axum_07` then `curl localhost:3000/player/7`.

use axum07 as axum;

use std::convert::Infallible;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
    response::IntoResponse,
    routing::get,
    Router,
};
use hewn::Wire;

// ---------------------------------------------------------------------------
// The leaf and the context. The axum State *is* the context, so the leaf is
// taught to wire from it directly.
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
// The `Wired<S>` extractor. On axum 0.7 `FromRequestParts` is an
// `#[async_trait]`, so the blanket carries the attribute. It works for every
// `S: Wire<State>` without per-service glue.
// ---------------------------------------------------------------------------

struct Wired<S>(pub S);

#[async_trait::async_trait]
impl<State, S> FromRequestParts<State> for Wired<S>
where
    State: Send + Sync,
    S: Wire<State>,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &State,
    ) -> Result<Self, Self::Rejection> {
        Ok(Wired(S::wire(state)))
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
        db: Db {
            url: "pg://localhost",
        },
    };

    let app: Router = Router::new()
        .route("/player/:id", get(get_player))
        .with_state(ctx);

    // Serve only when asked; otherwise just prove it wires and type-checks.
    if std::env::var_os("HEWN_SERVE").is_some() {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    } else {
        let _ = app;
        println!("axum_07 example wired; set HEWN_SERVE=1 to actually serve");
    }
}
