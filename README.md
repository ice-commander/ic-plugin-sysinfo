# System Information

An Ice Commander plugin that shows what the machine is: host, system, CPU,
memory, swap, uptime, network addresses, and live CPU and memory charts.

## What it registers

- one header button on the left, priority 10, icon only, tooltip
  `sysinfo.title`; it opens the view `sysinfo`;
- the view `sysinfo`, a JSON document the host draws as a dialog (700 × 820,
  scrolls vertically). It answers `describe` only: no events, no close handler;
- nine SVG assets under the owner `ic-sysinfo-dlg`: the window icon, a
  motherboard, a task icon and six system logos;
- the fifteen catalogues in `src/sysinfo-dlg/locales/` (be, bg, cs, de, en, es,
  fr, hu, it, pl, ro, ru, sk, sr, uk).

It claims no extensions, mounts no filesystem and draws no panel. It needs a
host that accepts plugin assets; an unrecognised host gets
`IC_ERR_HOST_UNKNOWN`, an older one `IC_ERR_HOST_TOO_OLD`.

## What the window shows

- the logo of the system the plugin was built for (`std::env::consts::OS`)
  with the system name and version;
- **Hardware Specifications**: hostname, operating system and version,
  architecture, core count, CPU usage, uptime, memory (GB), swap (MB), and a
  table of network interfaces with one address per row. Loopback addresses are
  left out; rows are sorted by name, then address;
- **Activity Monitoring**: CPU and memory charts, the last 60 samples each,
  scaled 0–100 %.

The document sets `refresh_ms: 1000`, asking the host to describe it again
every second. The plugin runs no timer: each `describe` reads the machine and
appends one sample to each chart.

The memory, swap and uptime sentences are filled in by the plugin in the locale
from the call context, falling back to English, then to the key. Readings come
from `sysinfo` 0.29, interfaces from `network-interface` 1.1.

## Building

```sh
./build.sh          # release build, libraries collected into bin/
./test.sh           # cargo test --workspace
./deploy-local.sh   # copy bin/ libraries into the host's plugin folder
```

`deploy-local.sh` targets `~/Library/Application Support/ice-commander/plugins`
on macOS, `%APPDATA%/ice-commander/plugins` on Windows and
`${XDG_DATA_HOME:-~/.local/share}/ice-commander/plugins` elsewhere;
`IC_PLUGIN_DIR` overrides it. Then switch the plugin on in
**Settings → Plugins** and restart.

The version lives in `package.json`; `npm run gen-version` writes it into
`version.rs`. `ic-plugin-api` is taken from the `main` branch of
<https://github.com/ice-commander/plugin-api>.

## Known limitations

- The chart history is process-wide and is never reset: reopening the window
  continues the previous lines, and a sample is taken per `describe` call, not
  per second.
- With no non-loopback address the interfaces table is empty, with no
  placeholder.
- The catalogues carry `sysinfo.back_to_services` and
  `sysinfo.enable_auto_update` (a back button and an auto-update toggle); the
  view has neither, and nothing looks them up.
- The catalogues also carry 39 `common_forms.*` phrases (service, sync-strategy
  and FTP/SFTP/WebDAV form text). Nothing in this plugin looks them up, but
  they are registered with the host along with the rest.

## Licence

Code: MIT or Apache-2.0, at your option. Icons: see
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md). Contributions are taken
under the DCO; sign off with `git commit -s`.
