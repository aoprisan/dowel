// `#[wire(with = path)]` calls `path(ctx)` and adds no Wire bound.
use hewn::Wire;

struct Ctx {
    seed: u64,
}

struct Clock {
    now: u64,
}

fn make_clock(ctx: &Ctx) -> Clock {
    Clock { now: ctx.seed + 1 }
}

#[derive(Wire)]
struct Service {
    #[wire(with = make_clock)]
    clock: Clock,
}

fn main() {
    let s = Service::wire(&Ctx { seed: 41 });
    assert_eq!(s.clock.now, 42);
}
