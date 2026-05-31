// `[T; N]` wires N independent instances of the same service from one context,
// expanding to a monomorphized `core::array::from_fn` — no container, no alloc.
use dowel::Wire;

struct Ctx {
    seed: u32,
}

#[derive(PartialEq, Debug)]
struct Worker {
    id: u32,
}
impl Wire<Ctx> for Worker {
    fn wire(ctx: &Ctx) -> Self {
        Worker { id: ctx.seed }
    }
}

// An array of a wired service also wires as a field of a service.
#[derive(Wire)]
struct Pool {
    workers: [Worker; 3],
}

fn main() {
    let direct: [Worker; 3] = Wire::wire(&Ctx { seed: 7 });
    assert_eq!(direct, [Worker { id: 7 }, Worker { id: 7 }, Worker { id: 7 }]);

    let pool = Pool::wire(&Ctx { seed: 9 });
    assert_eq!(pool.workers, [Worker { id: 9 }, Worker { id: 9 }, Worker { id: 9 }]);
}
