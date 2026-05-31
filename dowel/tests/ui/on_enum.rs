// Wire is struct-only; deriving on an enum is a clear error.
use dowel::Wire;

#[derive(Wire)]
enum Bad {
    A,
    B,
}

fn main() {}
