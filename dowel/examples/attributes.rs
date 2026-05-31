//! The wiring vocabulary in one plain composition root — no web framework.
//!
//! `#[derive(Wire)]` threads one `&ctx` through a graph of concrete structs.
//! This example exercises every field attribute and the array impl together so
//! you can see, in a single `main`, exactly what each one expands to:
//!
//! - a plain field is wired: `<F as Wire<Ctx>>::wire(ctx)`,
//! - `#[wire(skip)]` uses `Default::default()` (no `Wire` bound),
//! - `#[wire(default = expr)]` uses the given expression (no `Wire` bound),
//! - `#[wire(with = path)]` calls `path(ctx)` (no `Wire` bound),
//! - `[T; N]` wires N independent instances via `core::array::from_fn`.
//!
//! Run: `cargo run --example attributes`

use dowel::{Context, Wire};

// ---------------------------------------------------------------------------
// Leaves. Cheap, clonable handles the context owns (rule 4). `#[derive(Context)]`
// teaches the context to wire each one out of itself — no hand-written leaf impls.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Db {
    url: &'static str,
}

#[derive(Clone, Copy)]
struct Clock {
    epoch: u64,
}

#[derive(Context)]
struct AppCtx {
    db: Db,
    clock: Clock,
}

// ---------------------------------------------------------------------------
// A leaf with no `Default` but a known init — the case `#[wire(default = ..)]`
// exists for. It is *not* wired from the context and needs no `Wire` impl.
// ---------------------------------------------------------------------------

struct Cache {
    capacity: usize,
}
impl Cache {
    fn with_capacity(capacity: usize) -> Self {
        Cache { capacity }
    }
}

// ---------------------------------------------------------------------------
// A field with `Default` — the case `#[wire(skip)]` covers.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Metrics {
    hits: u64,
}

// ---------------------------------------------------------------------------
// A `#[wire(with = ..)]` provider. It must be generic over the context so the
// service stays generic (rule 3); any bound it needs comes from the context's
// own leaves. Here it derives a request id from the clock leaf.
// ---------------------------------------------------------------------------

struct RequestId(u64);

fn next_request_id<C>(ctx: &C) -> RequestId
where
    Clock: Wire<C>,
{
    let clock = Clock::wire(ctx);
    RequestId(clock.epoch + 1)
}

// ---------------------------------------------------------------------------
// A wired service: its `db` field is itself `Wire<Ctx>` and threads down.
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct PlayerRepo {
    db: Db,
}

// ---------------------------------------------------------------------------
// The service that puts every attribute side by side.
// ---------------------------------------------------------------------------

#[derive(Wire)]
struct Service {
    // plain: wired from the context
    repo: PlayerRepo,
    // plain leaf: also satisfies the `Clock: Wire<Ctx>` bound the `with`
    // provider below needs — a `with` provider's bounds must come from the
    // service's own wired fields (rule 3 keeps the service generic over Ctx).
    clock: Clock,
    // skip: Default::default(), no bound
    #[wire(skip)]
    metrics: Metrics,
    // default = expr: known init for a leaf with no Default, no bound
    #[wire(default = Cache::with_capacity(128))]
    cache: Cache,
    // with = path: build via a generic provider, no bound
    #[wire(with = next_request_id)]
    request_id: RequestId,
    // [T; N]: N independent instances wired from the same context
    repos: [PlayerRepo; 3],
}

fn main() {
    let ctx = AppCtx {
        db: Db { url: "pg://localhost" },
        clock: Clock { epoch: 100 },
    };

    let svc = Service::wire(&ctx);

    assert_eq!(svc.repo.db.url, "pg://localhost");
    assert_eq!(svc.clock.epoch, 100); // plain leaf
    assert_eq!(svc.metrics.hits, 0); // Default
    assert_eq!(svc.cache.capacity, 128); // default = expr
    assert_eq!(svc.request_id.0, 101); // with = provider (epoch + 1)
    assert_eq!(svc.repos.len(), 3); // array
    assert!(svc.repos.iter().all(|r| r.db.url == "pg://localhost"));

    println!("wired: repo+{} array repos, cache cap {}, request id {}", svc.repos.len(), svc.cache.capacity, svc.request_id.0);
}
