//! `Wired<S>` — declare a dependency directly in an axum handler signature.
//!
//! The axum `State` *is* the composition root: it holds the one concrete
//! context. `Wired<S>` is an extractor that calls `S::wire(state)` for any
//! service `S: Wire<State>`, so a handler asks for exactly the slice of the
//! graph it needs and nothing more. Adding a dependency to a handler is always
//! the same edit: add a `Wired<X>` parameter.
//!
//! On axum 0.8 `FromRequestParts` is a native `async fn` trait (no
//! `#[async_trait]`), and the blanket impl over `Wired<S>` compiles cleanly —
//! `Wired` is our own newtype, so there is no coherence clash with axum's
//! blankets. See `examples/axum_cqrs.rs` for the same pattern plus CQRS
//! dispatch via `Handles<C>`, and `examples/axum_07.rs` for the pre-0.8
//! `#[async_trait]` form.
//!
//! Run: `HEWN_SERVE=1 cargo run --example axum` then `curl localhost:3000/player/7`.

use std::convert::Infallible;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
    response::IntoResponse,
    routing::get,
    Router,
};
use dowel::Wire;

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
// The `Wired<S>` extractor: `S::wire(state)` straight from the axum State,
// which serves as the context. The blanket impl works for every
// `S: Wire<State>` without per-service glue.
// ---------------------------------------------------------------------------

struct Wired<S>(pub S);

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
        .route("/player/{id}", get(get_player))
        .with_state(ctx);

    // Serve only when asked; otherwise just prove it wires and type-checks.
    if std::env::var_os("HEWN_SERVE").is_some() {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    } else {
        let _ = app;
        println!("axum example wired; set HEWN_SERVE=1 to actually serve");
    }
}
