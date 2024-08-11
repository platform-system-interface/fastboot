use fastboot::Fastboot;
use getopts::Options;
use usbio::UsbDevice;

fn usage(program: &str, opts: &Options) {
    let ver = env!("CARGO_PKG_VERSION");
    let brief = format!("Version: {ver}\nUsage: {program} [options]");
    println!("{}", opts.usage(&brief));
}

// SpacemiT K1x
const DEFAULT_VID: u16 = 0x361c;
const DEFAULT_PID: u16 = 0x1001;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = args[0].clone();
    let mut opts = Options::new();
    opts.optflag("h", "help", "Print help");
    opts.optopt("", "vid", "Vendor ID", "<hex>");
    opts.optopt("", "pid", "Product ID", "<hex>");
    opts.optopt("", "var", "Variable name", "<string>");

    let matches = opts.parse(&args[1..]).unwrap_or_else(|err| {
        eprintln!("{program} failed to parse arguments ({err})!");
        usage(&program, &opts);
        std::process::exit(-1);
    });

    if matches.opt_present("h") {
        usage(&program, &opts);
        std::process::exit(0);
    }

    let vid = match matches.opt_str("vid") {
        Some(v) => u16::from_str_radix(&v, 16).expect("Parsing vendor ID failed"),
        None => DEFAULT_VID,
    };
    let pid = match matches.opt_str("pid") {
        Some(v) => u16::from_str_radix(&v, 16).expect("Parsing product ID failed"),
        None => DEFAULT_PID,
    };

    let variable = match matches.opt_str("var") {
        Some(v) => v,
        None => "version".to_owned(),
    };

    let di = nusb::list_devices()
        .unwrap()
        .find(|d| d.vendor_id() == vid && d.product_id() == pid)
        .expect("Device not found, is it connected and in the right mode?");

    // NOTE: The Fastboot trait gets us the necessary operations on the device.
    let mut dev = UsbDevice::new(di);
    if let Ok(var) = dev.getvar(&variable) {
        println!("{variable}: {var}");
    } else {
        println!("Could not get {variable} :(");
    }
}
