# hewn

Zero-cost compile-time dependency wiring for Rust. One trait, one derive, one
way to express a dependency. No container, no `TypeId`, no dynamic dispatch — the
derive expands to exactly the constructor you would hand-write.

```rust
pub trait Wire<Ctx> {
    fn wire(ctx: &Ctx) -> Self;
}
```

A *service* is a struct. Its *dependencies* are its fields. `#[derive(Wire)]`
generates the impl that wires each field from a context.

```rust
use hewn::Wire;

// The composition root owns one concrete context.
struct AppCtx { db: Db }

// A leaf is a cheap, clonable handle taught to the context by hand.
#[derive(Clone)]
struct Db { url: &'static str }
impl Wire<AppCtx> for Db {
    fn wire(ctx: &AppCtx) -> Self { ctx.db.clone() }
}

// Services derive their wiring: every field is itself `Wire<Ctx>`.
#[derive(Wire)]
struct PlayerRepo { db: Db }

#[derive(Wire)]
struct PlayerService { repo: PlayerRepo }

let ctx = AppCtx { db: Db { url: "pg://" } };
let svc = PlayerService::wire(&ctx);
```

## Field attributes

- `#[wire(skip)]` — construct the field with `Default::default()`; adds no bound.
- `#[wire(with = path)]` — construct it with `path(ctx)`; adds no bound. Keep the
  service generic over `Ctx`, so the provider is generic too
  (`fn make<C>(ctx: &C) -> Field`); any bound it needs (e.g. `Seed: Wire<C>`)
  must come from the struct's own wired fields.

Every plain field type `F` gets a `where F: Wire<Ctx>` bound, so a forgotten leaf
impl is a compile error at the wiring site:

```text
error[E0277]: the trait bound `Db: Wire<AppCtx>` is not satisfied
```

That is the intended repair signal — add the leaf impl, don't paper over it.

## axum

`examples/axum.rs` shows a `Wired<S>` extractor that calls `S::wire(&ctx)` from
the axum `State` (the composition root), letting a handler declare exactly the
slice of the graph it needs:

```rust
async fn get_player(Wired(repo): Wired<PlayerRepo>, Path(id): Path<u64>) -> impl IntoResponse {
    repo.find(id)
}
```

## The rules

1. A dependency is a struct field of a *concrete* type — never `Arc<dyn Trait>`.
2. Construction belongs to `#[derive(Wire)]`; don't hand-write a re-wiring `new()`.
3. Services stay generic over `Ctx`; the final binary picks the concrete context.
4. Leaves are cheap, clonable handles (`Arc`-backed or `Copy`).
5. Singletons live in the leaf, not the graph — the graph does not deduplicate.

## License

Licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE) at your option.
