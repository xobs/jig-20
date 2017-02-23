extern crate termcolor;

mod cfti;
use std::{thread, time};
use std::io::Write;

use self::termcolor::{BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};

fn main() {
    let mut controller = cfti::controller::Controller::new().unwrap();

    // Add a simple logger to show us debug data.
    /*
    controller.listen_logs(|msg| {
        let mut stdout = StandardStream::stderr(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red))).unwrap();
        writeln!(&mut stdout, "DEBUG: {:?}", msg).unwrap();
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White))).unwrap();
    });
*/
    // Add a simple logger to show us debug data.
    let bufwtr = BufferWriter::stderr(ColorChoice::Always);
    controller.listen(move |msg| {
        let mut buffer = bufwtr.buffer();

        buffer.set_color(ColorSpec::new().set_fg(Some(Color::Red))).unwrap();
        write!(&mut buffer, "DEBUG: ").unwrap();
        buffer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow))).unwrap();
        writeln!(&mut buffer, "{:?}", msg).unwrap();
        buffer.set_color(ColorSpec::new().set_fg(Some(Color::White))).unwrap();
        bufwtr.print(&buffer).unwrap();
    });

    let test_set = cfti::TestSet::new("ltc-tests", &mut controller).unwrap();

    println!("Test set: {:?}", test_set);
    loop {
        if controller.should_exit() {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
    }
}
