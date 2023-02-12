use rppal::gpio::{Gpio, InputPin, Trigger, Level};
use std::time::{Duration, Instant};

const PIN: u8 = 17;
const MAX_ENTRIES: usize = 10;
const ERROR_MARGIN: f64 = 0.05;

const TIMEOUT_DURATION = Duration::from_secs(3);
const ZERO_DURATION = Duration::from_millis(0);

const TARGET_NORMAL_BEEP_DURATION: Duration = Duration::from_millis(250);
const TARGET_LONG_BEEP_DURATION: Duration = Duration::from_secs(2);

const BEEP_BOUNCE_MAX_DURATION: Duration = Duration::from_millis(50);
const INTER_BEEP_BOUNCE_MAX_DURATION: Duration = Duration::from_millis(300);

#[derive(Hash, Eq, PartialEq, Debug)]
enum Status {
  OnMains,
  OnBattery,
  LowOnBattery,
  NoLoadOnBattery,
  OverloadOrShortCircuitOnBattery,
  OverloadOrShortCircuitOnMains,
  AdvanceLowRuntimeOnMains,
  OverTemperatureOnMains,
  OverTemperatureOnBatteryOrInternalError,
  ReplaceBattery,
  Unknown,
}

const  STATUS_DESCRIPTIONS: HashMap<Status, &str> = vec![
  (Status::OnBattery, "On battery power, no issues detected"),
  (Status::LowOnBattery, "Low battery, power backup will shut down in 1 minute"),
  (Status::NoLoadOnBattery, "Battery saver mode is enabled and power load is below 30W, power backup will shut down in 2 minutes"),
  (Status::OverloadOrShortCircuitOnBattery, "Overload or short circuit has occured on battery power, power backup will shut down in 5 minutes"),
  (Status::OverloadOrShortCircuitOnMains, "Overload or short circuit has occured on mains power"),
  (Status::AdvanceLowRuntimeOnMains, "Battery is on mains power and will have low runtime if it has to shift to battery power"),
  (Status::OverTemperatureOnMains, "Battery is over temperature on mains power"),
  (Status::OnMains, "On mains power, no issues detected"),
  (Status::OverTemperatureOnBatteryOrInternalError, "Battery is either over temperature on battery power or an internal error has occured"),
  (Status::ReplaceBattery, "Battery needs replacement"),
  (Status::Unknown, "Appropriate state could not be detected"),
].into_iter().collect();

const STATUS_BEEP_DURATIONS = [
  (Status::OnBattery, [TARGET_NORMAL_BEEP_DURATION, Duration::from_secs(60)]),
  (Status::LowOnBattery, [TARGET_NORMAL_BEEP_DURATION, Duration::from_secs(1)]),
  (Status::NoLoadOnBattery, [TARGET_NORMAL_BEEP_DURATION, Duration::from_secs(10)]),
  (Status::OverloadOrShortCircuitOnBattery, [TARGET_NORMAL_BEEP_DURATION, Duration::from_secs(2)]),
  (Status::OverloadOrShortCircuitOnMains, [TARGET_LONG_BEEP_DURATION, Duration::from_secs(2)]),
  (Status::AdvanceLowRuntimeOnMains, [TARGET_LONG_BEEP_DURATION, Duration::from_secs(13)]),
  (Status::OverTemperatureOnMains, [TARGET_NORMAL_BEEP_DURATION, Duration::from_secs(4)]),
  (Status::OnMains, [ZERO_DURATION, TIMEOUT_DURATION]),
  (Status::OverTemperatureOnBatteryOrInternalError, [TIMEOUT_DURATION, ZERO_DURATION]),
  (Status::ReplaceBattery, [TARGET_LONG_BEEP_DURATION, Duration::from_secs(40)]),
];

fn main() {
  let gpio = Gpio::new().unwrap();
  let pin = gpio.get(PIN).unwrap().into_input();
  pin.set_interrupt(Trigger::Both).unwrap();
  
  let mut beep_durations = vec![];
  let mut inter_beep_durations = vec![];

  let mut current_beep_start_time: Option<Instant> = None;
  let mut last_beep_end_time: Option<Instant> = None;

  let mut last_status: Option<Status>  = None;

  loop {
    let level = pin.poll_interrupt(true, Some(TIMEOUT_DURATION)).unwrap();
    
    if let Some(level) = level {
      let now = Instant::now();

      if level == Level::Low {
        // Don't update last_beep_end_time if it was already set previously so that on detecting another subsequent beep end without detecting a beep start first,
        // the original beep end still gets considered as the beep end
        if let None = last_beep_end_time {
          last_beep_end_time = now;
        }

        // Detect beep end only if we had previously detected a beep start,
        // because we need to calculate the duration of the beep as the time difference between now (beep end) and current_beep_start_time
        if let Some(current_beep_start_time) = current_beep_start_time {
          let beep_duration = now.duration_since(current_beep_start_time);
          // If the beep end happened too quickly since the beep start then just ignore the last beep start
          if beep_duration > MAX_BOUNCE_DURATION {
            beep_durations.push(beep_duration);
            if beep_durations.len() > MAX_ENTRIES {
              beep_durations.remove(0);
            }

            // After every detected beep, check for patterns and report the possible power state
            if !beep_durations.is_empty() && !inter_beep_durations.is_empty() {
              update_and_report_status(get_status_from_beep_durations(beep_durations.last().unwrap(), inter_beep_durations.last().unwrap()));
            }

          } else if inter_beep_durations.len() > 0 {
            inter_beep_durations.pop();
          }

          // Reset the current_beep_start_time variable to prevent detecting another subsequent beep end without detecting a beep start first,
          current_beep_start_time = None;
        }
      } else {
        // Don't update current_beep_start_time if it was already set previously so that on detecting another subsequent beep start without detecting a beep end first,
        // the original beep start still gets considered as the beep start
        if let None = current_beep_start_time {
          current_beep_start_time = now;
        }

        // Detect beep start only if we had previously detected a beep end,
        // because we need to calculate the duration between this and the last beep as the time difference between now (beep start) and  last_beep_end_time
        if let Some(last_beep_end_time) = last_beep_end_time {
          let inter_beep_duration = now.duration_since(last_beep_end_time);
          // If the beep start happened too quickly since the beep end then just ignore the last beep end
          if inter_beep_duration > MAX_BOUNCE_DURATION {
            inter_beep_durations.push(inter_beep_duration);
            if inter_beep_durations.len() > MAX_ENTRIES {
              inter_beep_durations.remove(0);
            }
          } else if beep_durations.len() > 0 {
            beep_durations.pop();
          }

          // Reset the last_beep_end_time variable to prevent detecting another subsequent beep start without detecting a beep end first,
          last_beep_end_time = None;
        }
      }
    } else {
      // When a timeout happens waiting for an interrupt then also check for patterns and report the possible power state
      if !beep_durations.is_empty() && !inter_beep_durations.is_empty() {
        if let Some(current_beep_start_time) = current_beep_start_time && let None = last_beep_end_time {
          // Timeout happened during a beep
          update_and_report_status(get_status_from_beep_durations(TIMEOUT_DURATION, ZERO_DURATION));
        } else if let None = current_beep_start_time && let Some(last_beep_end_time) = last_beep_end_time {
          // Timeout did not happen during a beep
          update_and_report_status(get_status_from_beep_durations(ZERO_DURATION, TIMEOUT_DURATION));
        } else {
          // THis case should not be possible
          update_and_report_status(Status::Unknown);
        }
      }
    }
  }
}

fn update_and_report_status(new_status: Status) {
  if last_status != new_status {
      last_status = new_status;
      println!(STATUS_DESCRIPTIONS[last_status]);
  }
}

fn get_status_from_beep_durations(beep: Duration, inter_beep: Duration) -> Status {
  for status_beep_duration in STATUS_BEEP_DURATIONS {
      if (
        close_enough(beep, status_beep_duration.1[0], BEEP_BOUNCE_MAX_DURATION) &&
        close_enough(inter_beep, status_beep_duration.1[1], INTER_BEEP_BOUNCE_MAX_DURATION)
       ) {
          return status_beep_duration.0;
      }
  }

  return Status::Unknown;
}

fn close_enough(duration: &Duration, target: Duration, error_margin: f64) -> bool {
  let error_range = target.as_micros() as f64 * error_margin;
  (duration.as_micros() as f64 - target.as_micros() as f64).abs() < error_range
}
