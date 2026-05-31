//! `Wired<S>` + CQRS on the latest axum (0.8) — native `async fn`, no
//! `async_trait`.
//!
//! This is the canonical shape for CLAUDE.md rule 6: when CQRS handlers are
//! involved, a command is dispatched **directly** through a `Handles<C>` trait.
//! The call `svc.handle(cmd)` is static and monomorphized — it compiles to a
//! plain method call, not a lookup. There is no command bus, no `dyn`, no
//! registry. Adding a new command is adding a `Handles<NewCmd>` impl; adding a
//! dependency to the handler is still just another `Wired<X>` parameter.
//!
//! Two things differ from axum 0.7 and are worth seeing:
//!   - `FromRequestParts` is a native `async fn` trait, so the `Wired<S>`
//!     blanket needs no `#[async_trait]`.
//!   - Path captures use brace syntax: `/player/{name}`, not `/player/:name`.
//!
//! Run: `HEWN_SERVE=1 cargo run --example axum_cqrs` then
//! `curl -X POST localhost:3000/player/ada`.

use std::convert::Infallible;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
    response::IntoResponse,
    routing::post,
    Router,
};
use dowel::Wire;

// ---------------------------------------------------------------------------
// Leaf + context. The axum State *is* the composition root.
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
// Services derive their wiring. Dependencies are concrete fields (rule 1).
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}
impl PlayerRepo {
    async fn insert(&self, name: &str) -> u64 {
        // Pretend to hit `self.db.url`; return a freshly minted id.
        let _ = self.db.url;
        let _ = name;
        7
    }
}

#[derive(Wire)]
struct PlayerService {
    repo: PlayerRepo,
}

// ---------------------------------------------------------------------------
// CQRS: one command, handled directly. `Handles<C>` is static dispatch — the
// monomorphized method call is the whole mechanism (rule 6). No bus.
//
// Native `async fn` in the trait is fine here precisely because we never need
// object safety: we always call `PlayerService: Handles<CreatePlayer>` by its
// concrete type. (A `dyn` bus boundary is the case that would need boxing — and
// rule 6 says don't reach for one by default.)
// ---------------------------------------------------------------------------

trait Handles<C> {
    type Output;
    async fn handle(&self, cmd: C) -> Self::Output;
}

struct CreatePlayer {
    name: String,
}

impl Handles<CreatePlayer> for PlayerService {
    type Output = u64;
    async fn handle(&self, cmd: CreatePlayer) -> u64 {
        self.repo.insert(&cmd.name).await
    }
}

// ---------------------------------------------------------------------------
// The `Wired<S>` extractor: `S::wire(state)` straight from the axum State. On
// axum 0.8 this is a native `async fn` impl. `Wired` is our own newtype, so the
// blanket has no coherence clash with axum's own `FromRequestParts` blankets.
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
// Handler: the dependency is declared in the signature; the command is built
// from the request and dispatched directly.
// ---------------------------------------------------------------------------

async fn create_player(
    Wired(svc): Wired<PlayerService>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let id = svc.handle(CreatePlayer { name }).await;
    format!("created player {id}")
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
        .route("/player/{name}", post(create_player))
        .with_state(ctx);

    // Serve only when asked; otherwise just prove it wires and type-checks.
    if std::env::var_os("HEWN_SERVE").is_some() {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    } else {
        let _ = app;
        println!("axum_cqrs example wired; set HEWN_SERVE=1 to actually serve");
    }
}
