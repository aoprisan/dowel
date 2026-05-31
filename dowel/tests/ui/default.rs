// `#[wire(default = expr)]` constructs the field with the given expression and
// adds no Wire bound — for leaves that have no `Default` but a known init.
use dowel::Wire;

struct Ctx;

// No `Default` impl: this is the case `#[wire(skip)]` can't cover.
struct Cache {
    capacity: usize,
}
impl Cache {
    fn with_capacity(capacity: usize) -> Self {
        Cache { capacity }
    }
}

#[derive(Wire)]
struct Service {
    #[wire(default = Cache::with_capacity(128))]
    cache: Cache,
}

fn main() {
    let s = Service::wire(&Ctx);
    assert_eq!(s.cache.capacity, 128);
}
