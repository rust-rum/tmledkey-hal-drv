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

use tmledkey_hal_drv::{self as tm, demo};

/**
 * Raspberry pi does not have open drain pins so we have to emulate it.
 * rppal unfortunately is not able to emulate such pins.
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

/**
 * rppal delays would not work because it use thread::sleep that does not provide accurate delays
 */
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

    let mut clk = gpio
        .get(clk_num)
        .expect("Was not able to get CLK pin")
        .into_output();

    let dio = gpio
        .get(dio_num)
        .expect("Was not able to get CLK pin")
        .into_io(Mode::Input);

    let mut tm_dio = OpenPin::new(dio);
    let mut delay = Delayer {};

    demo_2_wire_run(&mut tm_dio, &mut clk, &mut delay);
}

fn demo_2_wire_run<DIO, CLK, D>(dio: &mut DIO, clk: &mut CLK, delay: &mut D)
where
    DIO: InputPin + OutputPin,
    CLK: OutputPin,
    D: DelayMs<u16> + DelayUs<u16>,
{
    let delay_time = tm::TM1637_BUS_DELAY_US;

    println!("Starting 3 wire demo (TM1637)");
    let mut demo = demo::Demo::new(4);
    let init_res = demo.init_2wire(dio, clk, &mut |d: u16| delay.delay_us(d), delay_time);
    println!("Display initialized {:?}", init_res);

    let mut last_read = 0;
    loop {
        let read = demo.next_2wire(dio, clk, &mut |d: u16| delay.delay_us(d), delay_time);
        match read {
            Ok(byte) => {
                if byte != last_read {
                    last_read = byte;
                    println!("Key scan read: {:04b}_{:04b}", byte >> 4, byte & 0xF)
                }
            }
            Err(e) => {
                println!("Key scan read error {:?}", e);
            }
        };

        delay.delay_ms(75_u16);
    }
}

fn demo_3_wire_start(dio_num: u8, clk_num: u8, stb_num: u8) {
    let gpio = Gpio::new().expect("Can not init Gpio structure");

    let mut clk = gpio
        .get(clk_num)
        .expect("Was not able to get CLK pin")
        .into_output();

    let dio = gpio
        .get(dio_num)
        .expect("Was not able to get CLK pin")
        .into_io(Mode::Input);

    let mut stb = gpio
        .get(stb_num)
        .expect("Was not able to get STB pin")
        .into_output();

    let mut tm_dio = OpenPin::new(dio);

    let mut delayer = Delayer {};

    demo_3_wire_run(&mut tm_dio, &mut clk, &mut stb, &mut delayer);
}

fn demo_3_wire_run<DIO, CLK, STB, D>(dio: &mut DIO, clk: &mut CLK, stb: &mut STB, delay: &mut D)
where
    DIO: InputPin + OutputPin,
    CLK: OutputPin,
    STB: OutputPin,
    D: DelayMs<u16> + DelayUs<u16>,
{
    let delay_time = tm::TM1638_BUS_DELAY_US;

    println!("Starting 3 wire demo (TM1638)");
    let mut demo = demo::Demo::new(8);
    let init_res = demo.init_3wire(dio, clk, stb, &mut |d: u16| delay.delay_us(d), delay_time);
    println!("Display initialized {:?}", init_res);

    let mut last_read = [0_u8; 4];
    loop {
        let read = demo.next_3wire(dio, clk, stb, &mut |d: u16| delay.delay_us(d), delay_time);
        match read {
            Ok(bytes) => {
                if bytes != last_read {
                    last_read = bytes;
                    println!(
                        "Key scan read: {}",
                        last_read
                            .clone()
                            .into_iter()
                            .map(|b| format!("{:04b}_{:04b}", b >> 4, b & 0xF))
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
            }
            Err(e) => {
                println!("Key scan read error {:?}", e);
            }
        };

        delay.delay_ms(100_u16);
    }
}
