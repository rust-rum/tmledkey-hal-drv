extern crate clap;

use clap::{App, Arg, ArgMatches};

use embedded_hal::{
    blocking::delay::{DelayMs, DelayUs},
    digital::v2::{InputPin, OutputPin},
};
use rppal::gpio::{Gpio, IoPin, Mode};
use spin_sleep;
use std::time;
use void;

use tmledkey_hal_drv as tm;

/**
 * Raspberry pi does not have open drain pins so we have to emulate it.
 */
struct OpenPin {
    iopin: IoPin,
    mode: Mode,
}

impl OpenPin {
    fn new(mut pin: IoPin) -> OpenPin {
        pin.set_mode(Mode::Input);
        OpenPin {
            iopin: pin,
            mode: Mode::Input,
        }
    }

    fn switch_input(&mut self) {
        if self.mode != Mode::Input {
            self.mode = Mode::Input;
            self.iopin.set_mode(Mode::Input);
        }
    }

    fn switch_output(&mut self) {
        if self.mode != Mode::Output {
            self.mode = Mode::Output;
            self.iopin.set_mode(Mode::Output);
        }
    }
}

// Current rppal implementation does not support embedded_hal::gpio::v2 pins API.
impl InputPin for OpenPin {
    type Error = void::Void;

    fn is_high(&self) -> Result<bool, Self::Error> {
        Ok(self.iopin.is_high())
    }

    /// Is the input pin low?
    fn is_low(&self) -> Result<bool, Self::Error> {
        Ok(self.iopin.is_low())
    }
}

// Current rppal implementation does not support embedded_hal::gpio::v2 pins API.
impl OutputPin for OpenPin {
    type Error = void::Void;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.switch_output();
        self.iopin.set_low();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.iopin.set_high();
        self.switch_input();
        Ok(())
    }
}

struct OutPin {
    pin: rppal::gpio::OutputPin,
}
// Current rppal implementation does not support embedded_hal::gpio::v2 pins API.
impl OutputPin for OutPin {
    type Error = void::Void;

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.pin.set_low();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.pin.set_high();
        Ok(())
    }
}

fn cli_matches() -> ArgMatches<'static> {
    App::new("DHT tester")
        .author("Rumato Estorsky")
        .about("TM 163xx tests")
        .arg(
            Arg::with_name("clk")
                .long("clk")
                .value_name("PIN")
                .help("CLK pin number")
                .required(true),
        )
        .arg(
            Arg::with_name("dio")
                .long("dio")
                .value_name("PIN")
                .help("DIO pin number")
                .required(true),
        )
        .arg(
            Arg::with_name("stb")
                .long("stb")
                .value_name("PIN")
                .help("STB pin number for 3 wire interface")
                .required(false),
        )
        .get_matches()
}

struct Delayer;

impl DelayUs<u16> for Delayer {
    fn delay_us(&mut self, us: u16) {
        spin_sleep::sleep(time::Duration::from_micros(us as u64));
    }
}

impl DelayMs<u16> for Delayer {
    fn delay_ms(&mut self, ms: u16) {
        spin_sleep::sleep(time::Duration::from_millis(ms as u64));
    }
}

fn main() {
    let matches = cli_matches();

    let clk_num = matches
        .value_of("clk")
        .expect("Wrong CLK input")
        .parse::<u8>()
        .expect("Can not parse CLI as int");

    let dio_num = matches
        .value_of("dio")
        .expect("Wrong DIO input")
        .parse::<u8>()
        .expect("Can not parse DIO as int");
    let stb = matches.value_of("stb");

    println!(
        "Initialized using CLK:{} DIO:{}, STB:{:?}",
        clk_num, dio_num, stb
    );

    match stb {
        Some(sstb) => {
            let stb_num = sstb.parse::<u8>().expect("Can not parse STB as int");
            demo_3_wire_start(dio_num, clk_num, stb_num);
        }
        None => {
            demo_2_wire_start(dio_num, clk_num);
        }
    }
}

fn demo_2_wire_start(dio_num: u8, clk_num: u8) {
    let gpio = Gpio::new().expect("Can not init Gpio structure");

    let clk = gpio
        .get(clk_num)
        .expect("Was not able to get CLK pin")
        .into_output();

    let dio = gpio
        .get(dio_num)
        .expect("Was not able to get CLK pin")
        .into_io(Mode::Input);

    let mut tm_dio = OpenPin::new(dio);
    let mut tm_clk = OutPin { pin: clk };
    let mut delay = Delayer {};

    demo_2_wire_run(&mut tm_dio, &mut tm_clk, &mut delay);
}

fn demo_2_wire_run<DIO, CLK, D>(dio: &mut DIO, clk: &mut CLK, delayer: &mut D)
where
    DIO: InputPin + OutputPin,
    CLK: OutputPin,
    D: DelayMs<u16> + DelayUs<u16>,
{
    let mut bus_delay = |us: u16| delayer.delay_us(us);

    let r = tm::tm_send_bytes_2wire(
        dio,
        clk,
        &mut bus_delay,
        tm::TM1637_BUS_DELAY_US,
        &[tm::COM_DATA_ADDRESS_ADD],
    );
    println!("Display initialized: {:?}", r);

    let r = tm::tm_send_bytes_2wire(
        dio,
        clk,
        &mut bus_delay,
        tm::TM1637_BUS_DELAY_US,
        &[tm::COM_DISPLAY_ON],
    );
    println!("Brightness Init {:?}", r);

    let mut nums: [u8; 5] = [tm::COM_ADDRESS | 0, 1, 2, 3, 4];
    let mut iter = 0;
    loop {
        let mut bts: [u8; 5] = [0; 5];
        bts[0] = nums[0];
        for i in 1..nums.len() {
            bts[i] = tm::CHARS[(nums[i] as usize % tm::CHARS.len())];
        }

        let b = iter % 8;
        let r = tm::tm_send_bytes_2wire(
            dio,
            clk,
            &mut bus_delay,
            tm::TM1637_BUS_DELAY_US,
            &[tm::COM_DISPLAY_ON | b],
        );

        let pr = tm::tm_send_bytes_2wire(dio, clk, &mut bus_delay, tm::TM1637_BUS_DELAY_US, &bts);

        let read = tm::tm_read_byte_2wire(dio, clk, &mut bus_delay, tm::TM1637_BUS_DELAY_US);

        match read {
            Ok(byte) => println!("Byte readed: {:04b}_{:04b}", byte >> 4, byte & 0xF),
            Err(e) => println!("Read error {:?}", e),
        };

        spin_sleep::sleep(time::Duration::from_millis(400));
        for i in 1..nums.len() {
            nums[i] = nums[i] + 1;
        }
        iter += 1;
    }
}

fn demo_3_wire_start(dio_num: u8, clk_num: u8, stb_num: u8) {
    let gpio = Gpio::new().expect("Can not init Gpio structure");

    let clk = gpio
        .get(clk_num)
        .expect("Was not able to get CLK pin")
        .into_output();

    let dio = gpio
        .get(dio_num)
        .expect("Was not able to get CLK pin")
        .into_io(Mode::Input);

    let stb = gpio
        .get(stb_num)
        .expect("Was not able to get STB pin")
        .into_output();

    let mut tm_dio = OpenPin::new(dio);
    let mut tm_clk = OutPin { pin: clk };
    let mut tm_stb = OutPin { pin: stb };

    let mut delayer = Delayer {};

    demo_3_wire_run(&mut tm_dio, &mut tm_clk, &mut tm_stb, &mut delayer);
}

fn demo_3_wire_run<DIO, CLK, STB, D>(dio: &mut DIO, clk: &mut CLK, stb: &mut STB, delayer: &mut D)
where
    DIO: InputPin + OutputPin,
    CLK: OutputPin,
    STB: OutputPin,
    D: DelayMs<u16> + DelayUs<u16>,
{
    let mut bus_delay = |us: u16| delayer.delay_us(us);

    let r = tm::tm_send_bytes_3wire(
        dio,
        clk,
        stb,
        &mut bus_delay,
        tm::TM1638_BUS_DELAY_US,
        &[tm::COM_DATA_ADDRESS_ADD],
    );
    println!("Display initialized: {:?}", r);

    let r = tm::tm_send_bytes_3wire(
        dio,
        clk,
        stb,
        &mut bus_delay,
        tm::TM1638_BUS_DELAY_US,
        &[tm::COM_DISPLAY_ON],
    );
    println!("Brightness Inited {:?}", r);

    let mut nums: [u8; 9] = [tm::COM_ADDRESS | 0, 1, 2, 3, 4, 5, 6, 7, 8];
    let mut iter = 0;
    let mut key_scan: Vec<u8> = Vec::new();
    loop {
        let mut bts: [u8; 17] = [0; 17];
        bts[0] = nums[0];
        for i in 1..nums.len() {
            bts[(i - 1) * 2 + 1] = tm::CHARS[(nums[i] as usize % tm::CHARS.len())];
        }

        let b = iter % 8;
        let r = tm::tm_send_bytes_3wire(
            dio,
            clk,
            stb,
            &mut bus_delay,
            tm::TM1638_BUS_DELAY_US,
            &[tm::COM_DISPLAY_ON | b],
        );

        let pr =
            tm::tm_send_bytes_3wire(dio, clk, stb, &mut bus_delay, tm::TM1638_BUS_DELAY_US, &bts);

        let read = tm::tm_read_bytes_3wire(
            dio,
            clk,
            stb,
            &mut bus_delay,
            tm::TM1638_BUS_DELAY_US,
            tm::TM1638_RESPONSE_SIZE,
        );

        match read {
            Ok(bytes) => {
                if bytes != key_scan {
                    key_scan = bytes;

                    println!(
                        "Bytes read: {}",
                        key_scan
                            .clone()
                            .into_iter()
                            .map(|b| format!("{:04b}_{:04b}", b >> 4, b & 0xF))
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
            }
            Err(e) => println!("Read error {:?}", e),
        };

        spin_sleep::sleep(time::Duration::from_millis(400));
        for i in 1..nums.len() {
            nums[i] = nums[i] + 1;
        }
        iter += 1;
    }
}
