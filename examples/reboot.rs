use fastboot::Fastboot;
use getopts::Options;
use usbio::UsbDevice;

fn usage(program: &str, opts: &Options) {
    let ver = env!("CARGO_PKG_VERSION");
    let brief = format!("Version: {ver}\nUsage: {program} [options]");
    println!("{}", opts.usage(&brief));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = args[0].clone();
    let mut opts = Options::new();
    opts.optflag("h", "help", "Print help");
    opts.optopt("", "vid", "Vendor ID", "<hex>");
    opts.optopt("", "pid", "Product ID", "<hex>");

    if args.len() <= 1 {
        usage(&program, &opts);
        return;
    }
    let matches = opts.parse(&args[1..]).unwrap_or_else(|err| {
        eprintln!("{} failed to parse arguments ({})!", &program, err);
        usage(&program, &opts);
        std::process::exit(-1);
    });

    if matches.opt_present("h") {
        usage(&program, &opts);
        std::process::exit(0);
    }

    let vid = match matches.opt_str("vid") {
        Some(value) => u16::from_str_radix(&value, 16).expect("Parsing vendor ID failed"),
        None => 0x0451,
    };
    let pid = match matches.opt_str("pid") {
        Some(value) => u16::from_str_radix(&value, 16).expect("Parsing product ID failed"),
        None => 0xd022,
    };

    let di = nusb::list_devices()
        .unwrap()
        .find(|d| d.vendor_id() == vid && d.product_id() == pid)
        .expect("Device not found, is it connected and in the right mode?");
    let mut dev = UsbDevice::new(di);

    // NOTE: The Fastboot trait gets us the necessary operations on the device.
    println!("Rebooting: {:?}", dev.reboot());
}
