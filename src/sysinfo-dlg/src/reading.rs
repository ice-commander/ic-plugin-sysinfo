use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use sysinfo::{CpuExt, CpuRefreshKind, RefreshKind, System, SystemExt};

pub const HISTORY_LENGTH: usize = 60;

fn shared() -> &'static Mutex<System> {
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        Mutex::new(System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(),
        ))
    })
}

fn history() -> &'static Mutex<(Vec<f64>, Vec<f64>)> {
    static HISTORY: OnceLock<Mutex<(Vec<f64>, Vec<f64>)>> = OnceLock::new();
    HISTORY.get_or_init(|| Mutex::new((Vec::new(), Vec::new())))
}

pub fn remember(cpu: f64, memory: f64) -> (Vec<f64>, Vec<f64>) {
    fn append(line: &mut Vec<f64>, sample: f64) {
        line.push(sample);
        if line.len() > HISTORY_LENGTH {
            let extra = line.len() - HISTORY_LENGTH;
            line.drain(0..extra);
        }
    }
    let mut held = history().lock().unwrap_or_else(|kept| kept.into_inner());
    append(&mut held.0, cpu);
    append(&mut held.1, memory);
    (held.0.clone(), held.1.clone())
}

pub fn gigabytes(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

pub fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

pub fn split_uptime(seconds: u64) -> (u64, u64, u64) {
    (
        seconds / 86_400,
        (seconds % 86_400) / 3600,
        (seconds % 3600) / 60,
    )
}

pub fn interfaces() -> Vec<Value> {
    use network_interface::{NetworkInterface, NetworkInterfaceConfig};
    let mut found = Vec::new();
    let Ok(all) = NetworkInterface::show() else {
        return found;
    };
    for interface in all {
        for entry in interface.addr {
            let address = match entry {
                network_interface::Addr::V4(v4) => std::net::IpAddr::V4(v4.ip),
                network_interface::Addr::V6(v6) => std::net::IpAddr::V6(v6.ip),
            };
            if !address.is_loopback() {
                found.push(json!({
                    "name": interface.name.clone(),
                    "address": address.to_string(),
                }));
            }
        }
    }
    found.sort_by(|left, right| {
        let name = left["name"].as_str().unwrap_or_default();
        let other = right["name"].as_str().unwrap_or_default();
        name.cmp(other).then_with(|| {
            left["address"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["address"].as_str().unwrap_or_default())
        })
    });
    found
}

pub fn take() -> Value {
    let mut system = shared().lock().unwrap_or_else(|held| held.into_inner());
    system.refresh_cpu();
    system.refresh_memory();
    let share = if system.total_memory() > 0 {
        system.used_memory() as f64 / system.total_memory() as f64 * 100.0
    } else {
        0.0
    };
    let usage = f64::from(system.global_cpu_info().cpu_usage());
    // sysinfo divides 0 by 0 when two CPU refreshes land within one tick.
    let usage = if usage.is_nan() { 0.0 } else { usage };
    let (cpu_history, memory_history) = remember(usage, share);
    json!({
        "hostname": system.host_name().unwrap_or_else(|| "N/A".to_string()),
        "os_name": system.name().unwrap_or_else(|| "N/A".to_string()),
        "os_version": system.os_version().unwrap_or_else(|| "N/A".to_string()),
        "arch": std::env::consts::ARCH,
        "cores": system.cpus().len(),
        "cpu_usage": usage,
        "used_memory": system.used_memory(),
        "total_memory": system.total_memory(),
        "used_swap": system.used_swap(),
        "total_swap": system.total_swap(),
        "uptime": system.uptime(),
        "interfaces": interfaces(),
        "cpu_history": [ {
            "label": { "tr": "sysinfo.cpu_history", "en": "CPU History" },
            "color": [0.2, 0.6, 1.0, 0.8],
            "values": cpu_history
        } ],
        "mem_history": [ {
            "label": { "tr": "sysinfo.mem_history", "en": "Memory History" },
            "color": [0.9, 0.3, 0.5, 0.8],
            "values": memory_history
        } ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forget_history() {
        let mut held = history().lock().unwrap_or_else(|kept| kept.into_inner());
        held.0.clear();
        held.1.clear();
    }

    #[test]
    fn bytes_convert_to_the_units_the_window_shows() {
        assert_eq!(gigabytes(0), 0.0);
        assert_eq!(gigabytes(8 * 1024 * 1024 * 1024), 8.0);
        assert_eq!(megabytes(512 * 1024 * 1024), 512.0);
    }

    #[test]
    fn uptime_splits_into_the_three_units_the_phrases_expect() {
        assert_eq!(split_uptime(45), (0, 0, 0));
        assert_eq!(split_uptime(3 * 3600 + 25 * 60), (0, 3, 25));
        assert_eq!(split_uptime(2 * 86_400 + 3600 + 120), (2, 1, 2));
    }

    #[test]
    fn a_reading_carries_raw_facts_rather_than_sentences() {
        let reading = take();
        for key in ["hostname", "os_name", "os_version", "arch"] {
            assert!(
                reading.get(key).and_then(|value| value.as_str()).is_some(),
                "{key} is missing from the reading"
            );
        }
        for key in [
            "cores",
            "cpu_usage",
            "used_memory",
            "total_memory",
            "used_swap",
            "total_swap",
            "uptime",
        ] {
            assert!(
                reading.get(key).and_then(|value| value.as_f64()).is_some(),
                "{key} must come back as a number the presentation can format"
            );
        }
        assert!(reading.get("interfaces").expect("declared").is_array());
    }

    #[test]
    fn the_history_keeps_only_the_last_minute_of_samples() {
        forget_history();
        for sample in 0..(HISTORY_LENGTH + 20) {
            remember(sample as f64, sample as f64 / 2.0);
        }
        let (cpu, memory) = remember(1.0, 2.0);
        assert_eq!(cpu.len(), HISTORY_LENGTH);
        assert_eq!(memory.len(), HISTORY_LENGTH);
        assert_eq!(cpu.last(), Some(&1.0), "the newest sample is the last one");
        assert_eq!(memory.last(), Some(&2.0));
    }

    #[test]
    fn a_reading_carries_a_history_for_both_lines() {
        let reading = take();
        for key in ["cpu_history", "mem_history"] {
            let series = reading[key].as_array().expect("a series array");
            assert_eq!(series.len(), 1);
            assert!(series[0]["values"].as_array().expect("samples").len() <= HISTORY_LENGTH);
            assert_eq!(series[0]["color"].as_array().expect("a colour").len(), 4);
        }
    }

    #[test]
    fn interfaces_come_back_in_a_settled_order_so_the_list_stops_shuffling() {
        let once = interfaces();
        let twice = interfaces();
        assert_eq!(once, twice, "two readings must agree on the order");
        let names: Vec<&str> = once
            .iter()
            .map(|row| row["name"].as_str().unwrap_or_default())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "interfaces are listed by name");
        for pair in once.windows(2) {
            let (left, right) = (&pair[0], &pair[1]);
            if left["name"] == right["name"] {
                assert!(
                    left["address"].as_str().unwrap_or_default()
                        <= right["address"].as_str().unwrap_or_default(),
                    "addresses of one interface are listed in order too"
                );
            }
        }
    }

    #[test]
    fn a_loopback_address_is_never_offered_as_an_interface() {
        for row in interfaces() {
            let address = row["address"].as_str().expect("an address");
            assert_ne!(address, "127.0.0.1");
            assert_ne!(address, "::1");
        }
    }
}
