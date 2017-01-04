mod cfti;

fn main() {
    println!("Hello, world!");
    let tests = cfti::testentry::read_dir("ltc-tests");
    println!("Tests: {:?}", tests);
}
