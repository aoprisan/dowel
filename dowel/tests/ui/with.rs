// `#[wire(with = path)]` calls `path(ctx)` and adds no Wire bound for the field
// itself. To keep the service generic over the context (rule 3), the provider is
// generic over the context; any bound it needs (here `Seed: Wire<C>`) must be
// satisfiable from the struct's own wired fields — `seed` below supplies exactly
// that bound on the generated impl.
use dowel::Wire;

struct Ctx {
    seed: u64,
}

#[derive(Clone, Copy)]
struct Seed(u64);
impl Wire<Ctx> for Seed {
    fn wire(ctx: &Ctx) -> Self {
        Seed(ctx.seed)
    }
}

struct Clock {
    now: u64,
}

// Generic over the context; its `Seed: Wire<C>` bound is met by the wired
// `seed` field's generated bound.
fn make_clock<C>(ctx: &C) -> Clock
where
    Seed: Wire<C>,
{
    Clock {
        now: Seed::wire(ctx).0 + 1,
    }
}

#[derive(Wire)]
struct Service {
    seed: Seed,
    #[wire(with = make_clock)]
    clock: Clock,
}

fn main() {
    let s = Service::wire(&Ctx { seed: 41 });
    assert_eq!(s.seed.0, 41);
    assert_eq!(s.clock.now, 42);
}
