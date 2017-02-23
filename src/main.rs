extern crate termcolor;
extern crate clap;

mod cfti;
use std::{thread, time};
use std::io::Write;

use self::termcolor::{BufferWriter, Color, ColorChoice, ColorSpec, WriteColor};
use clap::{Arg, App};

fn main() {
    let mut config = cfti::config::Config::new();
    let matches = App::new("Jig-20 Test Controller")
                        .version("1.0")
                        .author("Sean Cross <sean@xobs.io>")
                        .about("Orchestrates the Common Factory Test Interface server")
                        .arg(Arg::with_name("LOCALE")
                            .short("l")
                            .long("language")
                            .value_name("LOCALE")
                            .help("Sets the language to the given locale, such as en_US, zh_CN, or zh")
                        )
                        .arg(Arg::with_name("TIMEOUT")
                            .short("t")
                            .long("timeout")
                            .value_name("SECONDS")
                            .default_value("10")
                            .help("The maximum number of seconds to allow individual test commands to run")
                        )
                        .arg(Arg::with_name("SCENARIO_TIMEOUT")
                            .short("s")
                            .long("scenario-timeout")
                            .value_name("SECONDS")
                            .default_value("60")
                            .help("The default number of seconds to allow scenarios to run, if unspecified")
                        )
                        .get_matches();

    config.set_locale(matches.value_of("LOCALE"));
    config.set_timeout(matches.value_of("TIMEOUT").unwrap().parse().unwrap());

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

    let test_set = cfti::TestSet::new("ltc-tests", &config, &mut controller).unwrap();

    println!("Test set: {:?}", test_set);
    loop {
        if controller.should_exit() {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
    }
}
