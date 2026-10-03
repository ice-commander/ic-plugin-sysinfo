mod reading;

use ic_plugin_api::{
    check_host, HostCheck, IcBytes, IcHost, IC_ABI_VERSION, IC_ERR_HOST_TOO_OLD,
    IC_ERR_HOST_UNKNOWN, IC_ERR_INIT_FAILED, IC_SIDE_LEFT,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};

pub const ID: &str = "ic-sysinfo-dlg";
pub const VIEW_ID: &str = "sysinfo";
const ICON: &str = include_str!("../assets/sysinfo.svg");

static HOST: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static ANSWER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub const ASSETS: &[(&str, &str)] = &[
    ("sysinfo", ICON),
    ("motherboard", include_str!("../assets/motherboard.svg")),
    ("system-task", include_str!("../assets/system-task.svg")),
    ("os-win", include_str!("../assets/os-win.svg")),
    ("os-mac", include_str!("../assets/os-mac.svg")),
    ("os-android", include_str!("../assets/os-android.svg")),
    ("os-iphone", include_str!("../assets/os-iphone.svg")),
    ("os-linux", include_str!("../assets/os-linux.svg")),
    ("os-console", include_str!("../assets/os-console.svg")),
];

const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.json")),
    ("ru", include_str!("../locales/ru.json")),
    ("pl", include_str!("../locales/pl.json")),
    ("cs", include_str!("../locales/cs.json")),
    ("sk", include_str!("../locales/sk.json")),
    ("de", include_str!("../locales/de.json")),
    ("es", include_str!("../locales/es.json")),
    ("uk", include_str!("../locales/uk.json")),
    ("it", include_str!("../locales/it.json")),
    ("fr", include_str!("../locales/fr.json")),
    ("ro", include_str!("../locales/ro.json")),
    ("hu", include_str!("../locales/hu.json")),
    ("be", include_str!("../locales/be.json")),
    ("bg", include_str!("../locales/bg.json")),
    ("sr", include_str!("../locales/sr.json")),
];

type Catalogue = std::collections::HashMap<String, std::collections::HashMap<String, String>>;

fn catalogues() -> &'static Catalogue {
    static PARSED: std::sync::OnceLock<Catalogue> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        LOCALES
            .iter()
            .filter_map(|(language, raw)| {
                serde_json::from_str(raw)
                    .ok()
                    .map(|table| (language.to_string(), table))
            })
            .collect()
    })
}

pub fn phrase(locale: &str, key: &str) -> String {
    let tables = catalogues();
    tables
        .get(locale)
        .and_then(|table| table.get(key))
        .or_else(|| tables.get("en").and_then(|table| table.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

pub fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut filled = template.to_string();
    for (name, value) in values {
        filled = filled.replace(&format!("%{{{name}}}"), value);
    }
    filled
}

pub fn os_asset(family: &str) -> &'static str {
    let lowered = family.to_lowercase();
    if lowered.contains("win") {
        "os-win"
    } else if lowered.contains("mac") || lowered.contains("darwin") {
        "os-mac"
    } else if lowered.contains("android") {
        "os-android"
    } else if lowered.contains("ios") || lowered.contains("iphone") {
        "os-iphone"
    } else if lowered.contains("linux") {
        "os-linux"
    } else {
        "os-console"
    }
}

fn section(id: &str, asset: &str, key: &str, english: &str) -> Value {
    json!({
        "t": "row",
        "id": id,
        "spacing": 12,
        "margin_top": 24,
        "children": [
            { "t": "icon", "height": 40, "icon": format!("asset:{ID}/{asset}") },
            { "t": "text", "role": "title2",
              "text": { "tr": format!("sysinfo.{key}"), "en": english } }
        ]
    })
}

fn reading_row(key: &str, english: &str) -> Value {
    json!({
        "t": "row",
        "id": format!("row_{key}"),
        "spacing": 8,
        "children": [
            { "t": "text", "width": 220, "role": "dim",
              "text": { "tr": format!("sysinfo.{key}"), "en": english } },
            { "t": "text", "id": key, "selectable": true, "weight": 1,
              "text": format!("{{data.{key}}}") }
        ]
    })
}

fn interfaces_row() -> Value {
    json!({
        "t": "row",
        "id": "network_row",
        "spacing": 8,
        "margin_top": 8,
        "children": [
            { "t": "text", "width": 220, "role": "dim",
              "text": { "tr": "sysinfo.network_interfaces", "en": "Network Interfaces:" } },
            { "t": "table", "id": "interfaces", "rows_key": "interfaces",
              "selectable": true, "weight": 1,
              "columns": [ { "key": "name", "width": 200 }, { "key": "address" } ] }
        ]
    })
}

fn specifications() -> Value {
    json!({
        "t": "column",
        "id": "specs",
        "spacing": 8,
        "margin_top": 16,
        "children": [
            reading_row("hostname", "Hostname:"),
            reading_row("os", "Operating System:"),
            reading_row("arch", "Architecture:"),
            reading_row("cores", "CPU Cores:"),
            reading_row("cpu_usage", "CPU Usage:"),
            reading_row("uptime", "Uptime:"),
            reading_row("memory", "Memory:"),
            reading_row("swap", "Swap:"),
            interfaces_row()
        ]
    })
}

pub fn document(locale: &str) -> Value {
    let reading = reading::take();
    let number = |key: &str| reading[key].as_u64().unwrap_or(0);
    let text = |key: &str| reading[key].as_str().unwrap_or("N/A").to_string();

    let memory = fill(
        &phrase(locale, "sysinfo.memory_format"),
        &[
            (
                "used",
                format!("{:.2}", reading::gigabytes(number("used_memory"))),
            ),
            (
                "total",
                format!("{:.2}", reading::gigabytes(number("total_memory"))),
            ),
        ],
    );
    let swap = fill(
        &phrase(locale, "sysinfo.swap_format"),
        &[
            (
                "used",
                format!("{:.2}", reading::megabytes(number("used_swap"))),
            ),
            (
                "total",
                format!("{:.2}", reading::megabytes(number("total_swap"))),
            ),
        ],
    );
    let (days, hours, minutes) = reading::split_uptime(number("uptime"));
    let uptime = if days > 0 {
        fill(
            &phrase(locale, "sysinfo.uptime_days_format"),
            &[
                ("days", days.to_string()),
                ("hours", hours.to_string()),
                ("minutes", minutes.to_string()),
            ],
        )
    } else {
        fill(
            &phrase(locale, "sysinfo.uptime_hours_format"),
            &[
                ("hours", hours.to_string()),
                ("minutes", minutes.to_string()),
            ],
        )
    };

    json!({
        "schema": 1,
        "data": {
            "os_title": format!("{} {}", text("os_name"), text("os_version")),
            "hostname": text("hostname"),
            "os": format!("{} ({})", text("os_name"), text("os_version")),
            "arch": text("arch"),
            "cores": number("cores").to_string(),
            "cpu_usage": format!("{:.1}%", reading["cpu_usage"].as_f64().unwrap_or(0.0)),
            "memory": memory,
            "swap": swap,
            "uptime": uptime,
            "interfaces": reading["interfaces"],
            "cpu_history": reading["cpu_history"],
            "mem_history": reading["mem_history"]
        },
        "fields": [],
        "form": {
            "t": "view",
            "surface": "dialog",
            "scroll": "vertical",
            "spacing": 12,
            "padding": 32,
            "width": 700,
            "height": 820,
            "refresh_ms": 1000,
            "children": [
                { "t": "row", "id": "hero", "spacing": 12, "children": [
                    { "t": "icon", "id": "os_logo", "height": 64,
                      "icon": format!("asset:{ID}/{}", os_asset(std::env::consts::OS)) },
                    { "t": "text", "id": "os_title", "role": "title1", "weight": 1,
                      "text": "{data.os_title}" } ] },

                section(
                    "specs_heading",
                    "motherboard",
                    "hardware_specs",
                    "Hardware Specifications"
                ),
                specifications(),

                section(
                    "activity_heading",
                    "system-task",
                    "activity_monitoring",
                    "Activity Monitoring"
                ),

                { "t": "text", "id": "cpu_history_heading", "role": "dim",
                  "text": { "tr": "sysinfo.cpu_history", "en": "CPU History" } },
                { "t": "chart", "id": "cpu_chart", "series_key": "cpu_history",
                  "height": 150, "min": 0, "max": 100, "caption": "100%" },

                { "t": "text", "id": "mem_history_heading", "role": "dim",
                  "text": { "tr": "sysinfo.mem_history", "en": "Memory History" } },
                { "t": "chart", "id": "mem_chart", "series_key": "mem_history",
                  "height": 150, "min": 0, "max": 100,
                  "caption": format!("{:.2} GB", reading::gigabytes(number("total_memory"))) }
            ]
        }
    })
}

fn locale_of(ctx: *const u8, ctx_len: u64) -> String {
    if ctx.is_null() || ctx_len == 0 {
        return "en".to_string();
    }
    let raw = unsafe { std::slice::from_raw_parts(ctx, ctx_len as usize) };
    let parsed: Value = serde_json::from_slice(raw).unwrap_or(Value::Null);
    parsed["host"]["locale"]
        .as_str()
        .unwrap_or("en")
        .to_string()
}

extern "C" fn describe(ctx: *const u8, ctx_len: u64, _user_data: *mut c_void) -> IcBytes {
    let source = document(&locale_of(ctx, ctx_len)).to_string();
    ANSWER.with(|slot| {
        *slot.borrow_mut() = source.into_bytes();
        let held = slot.borrow();
        IcBytes {
            data: held.as_ptr(),
            len: held.len() as u64,
        }
    })
}

extern "C" fn on_clicked(_user_data: *mut c_void, _parent: *mut c_void) {
    let host = HOST.load(Ordering::Relaxed) as *const IcHost;
    if host.is_null() {
        return;
    }
    let Ok(id) = CString::new(VIEW_ID) else {
        return;
    };
    unsafe {
        ((*host).open_view)(id.as_ptr(), std::ptr::null(), 0);
    }
}

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../version.rs"));

ic_plugin_api::declare_about!(
    "ic-sysinfo-dlg",
    "System Information",
    plugins_version!(),
    "Shows host, memory and load"
);

#[cfg_attr(feature = "export-abi", no_mangle)]
pub extern "C" fn ic_plugin_init(host: *const IcHost, _kind: *const c_char) -> c_int {
    match check_host(host, IC_ABI_VERSION, ic_plugin_api::needs_plugin_assets()) {
        HostCheck::Ok => {}
        HostCheck::WrongMagic => return IC_ERR_HOST_UNKNOWN,
        HostCheck::TooOld { .. } | HostCheck::Truncated { .. } => return IC_ERR_HOST_TOO_OLD,
    }
    HOST.store(host as usize, Ordering::Relaxed);
    for (name, svg) in ASSETS {
        let (Ok(owner), Ok(tag)) = (CString::new(ID), CString::new(*name)) else {
            continue;
        };
        unsafe {
            ((*host).register_plugin_asset)(
                owner.as_ptr(),
                tag.as_ptr(),
                svg.as_ptr(),
                svg.len() as u64,
            );
        }
    }
    for (language, catalogue) in LOCALES {
        let Ok(tag) = CString::new(*language) else {
            continue;
        };
        unsafe {
            ((*host).register_locales)(tag.as_ptr(), catalogue.as_ptr(), catalogue.len() as u64);
        }
    }
    let (Ok(id), Ok(svg), Ok(label), Ok(tooltip)) = (
        CString::new(VIEW_ID),
        CString::new(ICON),
        CString::new(""),
        CString::new("sysinfo.title"),
    ) else {
        return IC_ERR_INIT_FAILED;
    };
    let table = ic_plugin_api::IcViewVTable {
        struct_size: std::mem::size_of::<ic_plugin_api::IcViewVTable>() as u32,
        describe,
        on_event: None,
        closed: None,
    };
    let registered = unsafe {
        ((*host).register_view)(id.as_ptr(), tooltip.as_ptr(), &table, std::ptr::null_mut())
    };
    if registered != ic_plugin_api::IC_OK {
        return registered;
    }
    unsafe {
        ((*host).add_header_button)(
            id.as_ptr(),
            svg.as_ptr(),
            label.as_ptr(),
            tooltip.as_ptr(),
            IC_SIDE_LEFT,
            10,
            on_clicked,
            std::ptr::null_mut(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readings_are_spelled_as_sentences_with_units() {
        let document = document("en");
        let memory = document["data"]["memory"].as_str().expect("a sentence");
        assert!(memory.ends_with(" GB used"), "{memory}");
        assert!(memory.contains(" / "), "{memory}");
        let swap = document["data"]["swap"].as_str().expect("a sentence");
        assert!(swap.ends_with(" MB used"), "{swap}");
        let uptime = document["data"]["uptime"].as_str().expect("a sentence");
        assert!(uptime.contains("minutes"), "{uptime}");
        let usage = document["data"]["cpu_usage"].as_str().expect("a reading");
        assert!(usage.ends_with('%'), "{usage}");
        assert!(
            !memory.contains("%{"),
            "a placeholder survived into the window: {memory}"
        );
    }

    #[test]
    fn a_language_the_plugin_speaks_changes_the_sentences() {
        let english = phrase("en", "sysinfo.hostname");
        let russian = phrase("ru", "sysinfo.hostname");
        assert_ne!(english, russian, "the catalogues differ");
        assert_eq!(
            phrase("xx", "sysinfo.hostname"),
            english,
            "an unknown language falls back to english"
        );
        assert_eq!(phrase("en", "sysinfo.nothing"), "sysinfo.nothing");
    }

    #[test]
    fn the_logo_follows_the_system_it_is_running_on() {
        assert_eq!(os_asset("windows"), "os-win");
        assert_eq!(os_asset("macos"), "os-mac");
        assert_eq!(os_asset("linux"), "os-linux");
        assert_eq!(os_asset("android"), "os-android");
        assert_eq!(os_asset("ios"), "os-iphone");
        assert_eq!(os_asset("plan9"), "os-console");
    }

    #[test]
    fn the_plugin_ships_a_catalogue_for_every_language_the_app_speaks() {
        assert_eq!(LOCALES.len(), 15);
        for (language, catalogue) in LOCALES {
            let parsed: std::collections::HashMap<String, String> = serde_json::from_str(catalogue)
                .unwrap_or_else(|e| panic!("{language} is not a flat table: {e}"));
            assert!(
                parsed.contains_key("sysinfo.hostname"),
                "{language} is missing sysinfo.hostname"
            );
        }
    }

    #[test]
    fn the_plugin_refuses_a_host_it_does_not_recognise() {
        assert_eq!(
            ic_plugin_init(std::ptr::null(), std::ptr::null()),
            IC_ERR_HOST_UNKNOWN
        );
    }
}
