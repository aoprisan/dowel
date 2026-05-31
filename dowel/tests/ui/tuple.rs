// Tuple structs wire positionally, and the per-field attributes still apply.
use dowel::Wire;

struct Ctx {
    db: Db,
}

#[derive(Clone)]
struct Db {
    url: &'static str,
}
impl Wire<Ctx> for Db {
    fn wire(ctx: &Ctx) -> Self {
        ctx.db.clone()
    }
}

#[derive(Default)]
struct Cache;

#[derive(Wire)]
struct Pair(Db, #[wire(skip)] Cache);

fn main() {
    let ctx = Ctx {
        db: Db { url: "pg://" },
    };
    let p = Pair::wire(&ctx);
    assert_eq!(p.0.url, "pg://");
    let _ = p.1;
}
