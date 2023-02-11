use std::time::Instant;
use std::thread;
use std::sync::{Arc, Mutex};
use linux_embedded_hal::{Delay, Pin};
use rppal::gpio::{Gpio, Level};
use rppal::gpio::Interrupt;

fn main() {
    let gpio = Gpio::new().unwrap();
    let pin = gpio.get(25).unwrap().into_input_pullup();

    let mut beep_duration = 0;
    let mut gap_duration = 0;
    let mut last_beep = Instant::now();

    let data = Arc::new(Mutex::new((beep_duration, gap_duration, last_beep)));
    let data_clone = data.clone();

    pin.set_interrupt(Interrupt::BothEdges, move |level| {
        let mut data = data_clone.lock().unwrap();
        let now = Instant::now();

        match level {
            Level::High => {
                data.1 = now.duration_since(data.2).as_millis() as u32;
                data.2 = now;
            },
            Level::Low => {
                data.0 = now.duration_since(data.2).as_millis() as u32;
                data.2 = now;

                println!("Beep duration: {} ms, Gap duration: {} ms", data.0, data.1);

                if data.0 == 250 && data.1 == 1000 {
                    println!("Low battery detected");
                }
            },
        }
    });

    loop {
        thread::sleep(std::time::Duration::from_secs(60));
    }
}
