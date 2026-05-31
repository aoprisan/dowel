# CLAUDE.md — `hewn`

Zero-cost compile-time dependency wiring for Rust. This file governs both how
`hewn` itself is built and how consuming code (e.g. Dead City) must use it.

## What this crate is

A *convention*, not a framework. One trait, one derive, one way to express a
dependency. The goal is to remove choices so generated code is consistent and so
mistakes are compile errors, not runtime surprises. It compiles to exactly what
you would hand-write — no container, no `TypeId`, no dynamic dispatch.

```rust
pub trait Wire<Ctx> {
    fn wire(ctx: &Ctx) -> Self;
}
```

A service is a struct. Its dependencies are its fields. `#[derive(Wire)]`
generates the impl that wires each field from the context.

## The rules (non-negotiable — these are why the crate exists)

1. **A dependency is a struct field of a concrete type.** Never store
   `Arc<dyn Trait>` or `Box<dyn Trait>` as a dependency. If you reach for a trait
   object to express a dependency, stop — that is the pattern this crate replaces.
2. **Construction belongs to `#[derive(Wire)]`.** Do not hand-write a `new()` that
   duplicates wiring. If a field needs custom construction use `#[wire(with = ..)]`
   or `#[wire(skip)]`, not a bespoke constructor.
3. **Services are generic over `Ctx`.** Never hardcode a concrete context type into
   a library service. The final binary picks the context; library code stays
   `impl<Ctx> ... where Field: Wire<Ctx>`.
4. **Leaves are clonable handles.** Leaf dependencies (pools, clocks, senders) are
   `Clone` and cheap to clone — `Arc`-backed or `Copy`. This is load-bearing: it
   is what lets a wired service move into `tokio::spawn` with a `'static` bound.
5. **Singletons live in the leaf, not the graph.** The graph does not deduplicate.
   If one shared instance is required, the leaf is `Arc<Mutex<_>>` or an `mpsc`
   sender to a single owning task. Do not try to make the graph enforce uniqueness.
6. **One dispatch style per project.** If CQRS handlers are involved, call them
   directly via a `Handles<C>` trait (static, monomorphized). Do NOT introduce a
   dynamic command bus as the default dispatch path. A bus, if it exists at all, is
   an explicit isolated seam for one named reason (e.g. network boundary) and must
   be documented as the one dynamic exception.

## Canonical shapes (copy these, do not invent variants)

Leaf taught to the context:
```rust
#[derive(Clone)]
struct Db { pool: PgPool }
impl Wire<AppCtx> for Db { fn wire(c: &AppCtx) -> Self { c.db.clone() } }
```

Service:
```rust
#[derive(Wire)]
struct PlayerRepo { db: Db }
```

Composition root (the ONE place a concrete context exists):
```rust
let ctx = AppCtx { db, clock };
let repo = PlayerRepo::wire(&ctx);
```

Axum handler — declare the dependency in the signature via the `Wired<S>` extractor:
```rust
async fn get_player(Wired(repo): Wired<PlayerRepo>, Path(id): Path<PlayerId>) -> impl IntoResponse {
    repo.find(id).await
}
```

## When generating code that USES hewn

- Adding a dependency is always the same edit: add a field (service) or a
  `Wired<X>` parameter (handler). Never a new wiring decision.
- A missing wiring is a compile error like `the trait bound `Db: Wire<AppCtx>`
  is not satisfied`. That is intended — add the leaf impl, do not paper over it.
- Do not introduce `Arc<dyn>`, a runtime registry, or a `new()` that re-wires.

## Crate development (working on hewn itself)

- Workspace: `hewn` (facade) + `hewn-macros` (proc-macro: syn 2 / quote / proc-macro2).
- Edition 2021, stable only. No async in the wiring path.
- Facade crate has zero deps beyond the macro re-export.
- Every change must pass: `cargo check`, `cargo test`, `cargo clippy -- -D warnings`.
- Macro behavior is pinned by `trybuild` UI tests. The missing-dependency case
  MUST fail to compile with a readable message — treat its error text as a
  feature and review it on every macro change (it is Claude's repair signal).
- Before implementing or changing the macro, show the planned token expansion
  first, then implement.

## Known-unverified spots (verify with `cargo check`, do not assume)

- The `Wired<S>` axum extractor's blanket `FromRequestParts` impl may collide with
  axum's own blankets (coherence). If it fights, fall back to calling
  `S::wire(&ctx)` inside the handler body. Verify before relying on it.
- Object safety: `async fn` in a `Handles<C>` trait is NOT object-safe. A dynamic
  bus boundary needs `#[async_trait]` or `Pin<Box<dyn Future>>`. Only relevant if
  rule 6's bus exception is used.
- `hewn` name availability on crates.io is unconfirmed. Check before publishing.

## Tone for this repo

Terse, correct, compile-checked. When unsure whether something compiles, say so
and run `cargo check` rather than asserting it works.
