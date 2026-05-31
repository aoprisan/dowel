// `#[wire(skip)]` uses Default::default() and adds no Wire bound, so the field
// type need not be Wire<Ctx> at all.
use hewn::Wire;

struct Ctx;

#[derive(Default)]
struct Cache {
    hits: u64,
}

#[derive(Wire)]
struct Service {
    #[wire(skip)]
    cache: Cache,
}

fn main() {
    let s = Service::wire(&Ctx);
    assert_eq!(s.cache.hits, 0);
}
