# CLAUDE.md — `dowel`

Zero-cost compile-time dependency wiring for Rust. This file governs both how
`dowel` itself is built and how consuming code (e.g. Dead City) must use it.

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

Leaves taught in bulk — `#[derive(Context)]` generates one `impl Wire<AppCtx> for
FieldType` per field (clones the field out). Prefer this over a wall of
hand-written leaf impls; drop to the hand-written form only for a field that needs
custom logic:
```rust
#[derive(Context)]
struct AppCtx { db: Db, clock: Clock }
// `#[context(skip)]` omits a field; two fields of the same type is a compile error.
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

## When generating code that USES dowel

- Adding a dependency is always the same edit: add a field (service) or a
  `Wired<X>` parameter (handler). Never a new wiring decision.
- A missing wiring is a compile error like `the trait bound `Db: Wire<AppCtx>`
  is not satisfied`. That is intended — add the leaf impl, do not paper over it.
- Do not introduce `Arc<dyn>`, a runtime registry, or a `new()` that re-wires.

## Crate development (working on dowel itself)

- Workspace: `dowel` (facade) + `dowel-macros` (proc-macro: syn 2 / quote / proc-macro2).
- Edition 2021, stable only. No async in the wiring path.
- Facade crate has zero deps beyond the macro re-export.
- Every change must pass: `cargo check`, `cargo test`, `cargo clippy -- -D warnings`.
- Macro behavior is pinned by `trybuild` UI tests. The missing-dependency case
  MUST fail to compile with a readable message — treat its error text as a
  feature and review it on every macro change (it is Claude's repair signal).
- Before implementing or changing the macro, show the planned token expansion
  first, then implement.

## Settled decisions

- **The `Wired<S>` axum extractor stays an example, not facade code.** Its blanket
  `FromRequestParts` impl compiles cleanly (verified on axum 0.7 *and* 0.8) — no
  coherence clash, because `Wired` is our own newtype, so the feared collision
  with axum's blankets never materializes. We still keep it out of the facade on
  purpose: vendoring it would add an `axum` (framework) dependency and pin one
  axum major line, breaking the "facade has zero deps beyond the macro re-export"
  invariant. The canonical shape lives in `examples/axum.rs` (extractor, latest
  axum), `examples/axum_cqrs.rs` (extractor + CQRS), and `examples/axum_07.rs` (the
  pre-0.8 `#[async_trait]` form). The 0.7 example pulls axum in under a renamed
  package (`axum07 = { package = "axum", version = "0.7" }`) so both lines coexist
  in dev-deps. If a second consumer ever needs it installable, ship it as a separate
  `dowel-axum` companion crate that owns the axum-version coupling — never the facade.

## Known-unverified spots (verify with `cargo check`, do not assume)

- Object safety: `async fn` in a `Handles<C>` trait is NOT object-safe. That is
  fine for the default CQRS path — `examples/axum_cqrs.rs` dispatches commands by
  concrete type (`PlayerService: Handles<CreatePlayer>`), which is static and
  monomorphized. A dynamic bus boundary is the only case that would need
  `#[async_trait]` or `Pin<Box<dyn Future>>`, and rule 6 says don't default to one.
- `dowel` name is available on crates.io (confirmed). Claim it on first publish.

## Tone for this repo

Terse, correct, compile-checked. When unsure whether something compiles, say so
and run `cargo check` rather than asserting it works.
