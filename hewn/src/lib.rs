//! # hewn
//!
//! Zero-cost compile-time dependency wiring for Rust. One trait, one derive, one
//! way to express a dependency. There is no container, no `TypeId`, no dynamic
//! dispatch — the derive expands to exactly the constructor you would hand-write.
//!
//! A *service* is a struct. Its *dependencies* are its fields. [`derive@Wire`]
//! generates the [`Wire`] impl that wires each field from a context.
//!
//! ```
//! use hewn::Wire;
//!
//! // The composition root owns one concrete context.
//! struct AppCtx { db: Db }
//!
//! // A leaf is a cheap, clonable handle taught to the context by hand.
//! #[derive(Clone)]
//! struct Db { url: &'static str }
//! impl Wire<AppCtx> for Db {
//!     fn wire(ctx: &AppCtx) -> Self { ctx.db.clone() }
//! }
//!
//! // A service derives its wiring: every field is itself `Wire<Ctx>`.
//! #[derive(Wire)]
//! struct PlayerRepo { db: Db }
//!
//! let ctx = AppCtx { db: Db { url: "postgres://localhost" } };
//! let repo = PlayerRepo::wire(&ctx);
//! assert_eq!(repo.db.url, "postgres://localhost");
//! ```
//!
//! ## A three-level graph
//!
//! Wiring is recursive: a service whose fields are services whose fields are
//! leaves. Each level only needs its own fields to be `Wire<Ctx>`; the derive
//! threads the same `&ctx` all the way down.
//!
//! ```
//! use hewn::Wire;
//!
//! struct AppCtx { db: Db, clock: Clock }
//!
//! #[derive(Clone)]
//! struct Db { url: &'static str }
//! impl Wire<AppCtx> for Db { fn wire(c: &AppCtx) -> Self { c.db.clone() } }
//!
//! #[derive(Clone, Copy)]
//! struct Clock;
//! impl Wire<AppCtx> for Clock { fn wire(c: &AppCtx) -> Self { c.clock } }
//!
//! // Level 1: leaf -> repo
//! #[derive(Wire)]
//! struct PlayerRepo { db: Db }
//!
//! // Level 2: repo + leaf -> service
//! #[derive(Wire)]
//! struct PlayerService { repo: PlayerRepo, clock: Clock }
//!
//! let ctx = AppCtx { db: Db { url: "pg://" }, clock: Clock };
//! let svc = PlayerService::wire(&ctx);
//! assert_eq!(svc.repo.db.url, "pg://");
//! ```
//!
//! ## What the derive generates
//!
//! For a named struct
//!
//! ```ignore
//! #[derive(Wire)]
//! struct PlayerService<T> {
//!     repo: PlayerRepo,
//!     #[wire(skip)] cache: Cache,
//!     #[wire(with = make_clock)] clock: Clock,
//!     extra: T,
//! }
//! ```
//!
//! the macro expands to (modulo hygiene):
//!
//! ```ignore
//! impl<__Ctx, T> hewn::Wire<__Ctx> for PlayerService<T>
//! where
//!     PlayerRepo: hewn::Wire<__Ctx>,
//!     T: hewn::Wire<__Ctx>,
//! {
//!     fn wire(__ctx: &__Ctx) -> Self {
//!         Self {
//!             repo: <PlayerRepo as hewn::Wire<__Ctx>>::wire(__ctx),
//!             cache: ::core::default::Default::default(), // #[wire(skip)]
//!             clock: make_clock(__ctx),                   // #[wire(with = ..)]
//!             extra: <T as hewn::Wire<__Ctx>>::wire(__ctx),
//!         }
//!     }
//! }
//! ```
//!
//! Notes on the expansion:
//! - A fresh `__Ctx` type parameter is introduced so services stay generic over
//!   the context (rule 3). The struct's own generics are preserved verbatim,
//!   including any existing bounds and `where` clause.
//! - Each *wired* field type gets a `Field: Wire<__Ctx>` bound. A missing leaf
//!   impl therefore surfaces as `the trait bound `Db: Wire<AppCtx>` is not
//!   satisfied` at the wiring site — that is the intended repair signal.
//! - `#[wire(skip)]` fields are constructed with [`Default::default`] and get no
//!   bound. `#[wire(with = path)]` fields call `path(ctx)` and get no bound.
//! - Tuple structs expand identically using positional initializers
//!   (`Self(<F0 as Wire<__Ctx>>::wire(__ctx), ...)`).
//!
//! See the crate `README` and `examples/axum.rs` for the `Wired<S>` extractor
//! pattern used to declare a dependency directly in an axum handler signature.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]

/// Construct `Self` by wiring each dependency from a shared context.
///
/// Implement this by hand for *leaves* (pools, clocks, senders) — the handles
/// the context actually owns — and `#[derive(Wire)]` it for *services* whose
/// fields are themselves `Wire<Ctx>`.
///
/// ```
/// use hewn::Wire;
///
/// struct Ctx { name: String }
///
/// struct Greeting { who: String }
/// impl Wire<Ctx> for Greeting {
///     fn wire(ctx: &Ctx) -> Self {
///         Greeting { who: ctx.name.clone() }
///     }
/// }
///
/// let g = Greeting::wire(&Ctx { name: "world".into() });
/// assert_eq!(g.who, "world");
/// ```
pub trait Wire<Ctx> {
    /// Build `Self` from `ctx`. This is pure construction — no async, no I/O.
    fn wire(ctx: &Ctx) -> Self;
}

#[cfg(feature = "derive")]
pub use hewn_macros::Wire;
