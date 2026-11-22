//! CPU, memory, GPU, disk and network usage for the system indicator and its sub-panel.
//! `/proc` is read on every tick (cheap); `df` and `nvidia-smi` only while the sub-panel is open.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Network samples kept for the graph.
pub const HISTORY: usize = 60;
/// Usage at or above this percentage shows in red.
pub const HOT: f32 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    idle: u64,
    total: u64,
}

/// Bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl Memory {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }

    pub fn percent(&self) -> f32 {
        fraction(self.used(), self.total) * 100.0
    }
}

/// Bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gpu {
    pub percent: f32,
    pub temperature: f32,
    /// Bytes.
    pub memory_used: u64,
    pub memory_total: u64,
}

/// `used / total`, 0 when there is no total.
pub fn fraction(used: u64, total: u64) -> f32 {
    if total == 0 { 0.0 } else { (used as f64 / total as f64) as f32 }
}

/// The aggregate `cpu` line of `/proc/stat`; iowait counts as idle.
pub fn parse_cpu_times(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|line| line.starts_with("cpu "))?;
    // user nice system idle iowait irq softirq steal; guest time is already in user.
    let fields: Vec<u64> = line.split_whitespace().skip(1).take(8).filter_map(|field| field.parse().ok()).collect();
    if fields.len() < 5 {
        return None;
    }
    Some(CpuTimes { idle: fields[3] + fields[4], total: fields.iter().sum() })
}

/// Busy percentage between two `/proc/stat` readings.
pub fn cpu_percent(previous: CpuTimes, current: CpuTimes) -> f32 {
    let total = current.total.saturating_sub(previous.total);
    let idle = current.idle.saturating_sub(previous.idle);
    if total == 0 { 0.0 } else { (1.0 - fraction(idle, total)) * 100.0 }
}

pub fn parse_meminfo(meminfo: &str) -> Option<Memory> {
    let field = |key: &str| {
        meminfo
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
            .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
            .map(|kib| kib * 1024)
    };
    Some(Memory {
        total: field("MemTotal")?,
        available: field("MemAvailable")?,
        swap_total: field("SwapTotal").unwrap_or(0),
        swap_free: field("SwapFree").unwrap_or(0),
    })
}

/// Received and sent bytes of `interface` in `/proc/net/dev`.
pub fn parse_net_dev(net_dev: &str, interface: &str) -> Option<(u64, u64)> {
    let data = net_dev.lines().find_map(|line| {
        let (name, data) = line.split_once(':')?;
        (name.trim() == interface).then_some(data)
    })?;
    let fields: Vec<u64> = data.split_whitespace().filter_map(|field| field.parse().ok()).collect();
    Some((*fields.first()?, *fields.get(8)?))
}

/// Interface of the default route in `/proc/net/route`.
pub fn default_interface(route: &str) -> Option<String> {
    route.lines().skip(1).find_map(|line| {
        let mut fields = line.split_whitespace();
        let interface = fields.next()?;
        (fields.next()? == "00000000").then(|| interface.to_owned())
    })
}

/// Average clock of all cores in GHz, from `/proc/cpuinfo`.
pub fn parse_cpu_ghz(cpuinfo: &str) -> Option<f32> {
    let clocks: Vec<f32> = cpuinfo
        .lines()
        .filter(|line| line.starts_with("cpu MHz"))
        .filter_map(|line| line.split_once(':')?.1.trim().parse().ok())
        .collect();
    (!clocks.is_empty()).then(|| clocks.iter().sum::<f32>() / clocks.len() as f32 / 1000.0)
}

/// `df -B1 --output=target,size,used` output, header skipped; `/` first, then the largest.
pub fn parse_df(df: &str) -> Vec<Disk> {
    let mut disks: Vec<Disk> = df
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let mount = fields.next()?.to_owned();
            let total = fields.next()?.parse().ok()?;
            let used = fields.next()?.parse().ok()?;
            Some(Disk { mount, used, total })
        })
        .collect();
    disks.sort_by_key(|disk| (disk.mount != "/", std::cmp::Reverse(disk.total)));
    disks
}

/// `nvidia-smi --query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total
/// --format=csv,noheader,nounits` output (MiB); the first GPU.
pub fn parse_nvidia(csv: &str) -> Option<Gpu> {
    let fields: Vec<f32> = csv.lines().next()?.split(',').map(|field| field.trim().parse().ok()).collect::<Option<_>>()?;
    let mib = |value: f32| (value as u64) << 20;
    match fields[..] {
        [percent, temperature, used, total] => {
            Some(Gpu { percent, temperature, memory_used: mib(used), memory_total: mib(total) })
        }
        _ => None,
    }
}

/// `8.3 GiB`
pub fn format_gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64)
}

/// `12.4 KB/s`, `1.2 MB/s`, `0 B/s`.
pub fn format_rate(bytes_per_second: f32) -> String {
    match bytes_per_second {
        rate if rate < 1024.0 => format!("{rate:.0} B/s"),
        rate if rate < 1024.0 * 1024.0 => format!("{:.1} KB/s", rate / 1024.0),
        rate => format!("{:.1} MB/s", rate / (1024.0 * 1024.0)),
    }
}

/// What the indicator, the tooltip and the sub-panel show.
#[derive(Debug, Default)]
pub struct Usage {
    cpu_times: Option<CpuTimes>,
    net_bytes: Option<(u64, u64, Instant)>,
    pub cpu: f32,
    pub memory: Memory,
    pub cpu_temperature: Option<f32>,
    pub cpu_ghz: Option<f32>,
    pub interface: Option<String>,
    /// Bytes per second, oldest first.
    pub download: VecDeque<f32>,
    pub upload: VecDeque<f32>,
    pub disks: Vec<Disk>,
    pub gpu: Option<Gpu>,
}

impl Usage {
    /// Reads `/proc` and the CPU sensor.
    pub fn sample(&mut self, now: Instant) {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        self.interface = default_interface(&read("/proc/net/route"));
        self.cpu_temperature = cpu_temperature();
        self.cpu_ghz = parse_cpu_ghz(&read("/proc/cpuinfo"));
        self.update(&read("/proc/stat"), &read("/proc/meminfo"), &read("/proc/net/dev"), now);
    }

    /// Takes one reading of `/proc/stat`, `/proc/meminfo` and `/proc/net/dev`.
    pub fn update(&mut self, stat: &str, meminfo: &str, net_dev: &str, now: Instant) {
        if let Some(times) = parse_cpu_times(stat) {
            if let Some(previous) = self.cpu_times {
                self.cpu = cpu_percent(previous, times);
            }
            self.cpu_times = Some(times);
        }
        if let Some(memory) = parse_meminfo(meminfo) {
            self.memory = memory;
        }
        let bytes = self.interface.as_deref().and_then(|interface| parse_net_dev(net_dev, interface));
        if let (Some((received, sent)), Some((last_received, last_sent, at))) = (bytes, self.net_bytes) {
            let seconds = now.saturating_duration_since(at).as_secs_f32();
            if seconds > 0.0 {
                let rate = |now: u64, before: u64| now.saturating_sub(before) as f32 / seconds;
                push(&mut self.download, rate(received, last_received));
                push(&mut self.upload, rate(sent, last_sent));
            }
        }
        self.net_bytes = bytes.map(|(received, sent)| (received, sent, now));
    }

    pub fn set_details(&mut self, disks: Vec<Disk>, gpu: Option<Gpu>) {
        self.disks = disks;
        self.gpu = gpu;
    }
}

fn push(history: &mut VecDeque<f32>, value: f32) {
    if history.len() == HISTORY {
        history.pop_front();
    }
    history.push_back(value);
}

/// `k10temp`, `zenpower` or `coretemp` from hwmon, in °C.
fn cpu_temperature() -> Option<f32> {
    for entry in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let path = entry.path();
        let name = std::fs::read_to_string(path.join("name")).unwrap_or_default();
        if matches!(name.trim(), "k10temp" | "zenpower" | "coretemp") {
            let millis: f32 = std::fs::read_to_string(path.join("temp1_input")).ok()?.trim().parse().ok()?;
            return Some(millis / 1000.0);
        }
    }
    None
}

/// Disks and the GPU, read off the UI thread.
pub async fn details() -> (Vec<Disk>, Option<Gpu>) {
    let run = |program: &'static str, args: &'static [&'static str]| async move {
        let output = tokio::process::Command::new(program).args(args).kill_on_drop(true).output();
        match tokio::time::timeout(Duration::from_secs(3), output).await {
            Ok(Ok(output)) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
            _ => String::new(),
        }
    };
    let df = run("df", &["-B1", "--output=target,size,used", "-x", "tmpfs", "-x", "devtmpfs", "-x", "efivarfs", "-x", "overlay"]);
    let gpu = run(
        "nvidia-smi",
        &["--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total", "--format=csv,noheader,nounits"],
    );
    let (df, gpu) = tokio::join!(df, gpu);
    (parse_df(&df), parse_nvidia(&gpu))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\nintr 1\n";
    const STAT_LATER: &str = "cpu  160 0 70 900 50 0 0 0 0 0\n";
    const MEMINFO: &str = "MemTotal:       32722716 kB\nMemFree:         4000000 kB\nMemAvailable:   24000000 kB\nSwapTotal:       4194300 kB\nSwapFree:        4194300 kB\n";
    const NET_DEV: &str = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 5000 10 0 0 0 0 0 0 5000 10 0 0 0 0 0 0\nenp6s0: 1000000 900 0 0 0 0 0 0 200000 500 0 0 0 0 0 0\n";

    #[test]
    fn cpu_usage_comes_from_the_difference_of_two_readings() {
        let first = parse_cpu_times(STAT).unwrap();
        assert_eq!(first, CpuTimes { idle: 850, total: 1000 });
        let second = parse_cpu_times(STAT_LATER).unwrap();
        // 180 more jiffies, 100 of them idle.
        assert!((cpu_percent(first, second) - 44.444).abs() < 0.01);
        assert_eq!(cpu_percent(second, second), 0.0);
        assert_eq!(parse_cpu_times("intr 1"), None);
    }

    #[test]
    fn memory_used_is_total_minus_available() {
        let memory = parse_meminfo(MEMINFO).unwrap();
        assert_eq!(memory.total, 32722716 * 1024);
        assert_eq!(memory.used(), (32722716 - 24000000) * 1024);
        assert_eq!(memory.swap_used(), 0);
        assert!((memory.percent() - 26.66).abs() < 0.01);
        assert_eq!(parse_meminfo("MemFree: 1 kB"), None);
    }

    #[test]
    fn network_bytes_and_the_default_interface() {
        assert_eq!(parse_net_dev(NET_DEV, "enp6s0"), Some((1000000, 200000)));
        assert_eq!(parse_net_dev(NET_DEV, "wlan0"), None);
        let route = "Iface\tDestination\tGateway \tFlags\nenp6s0\t0000A8C0\t00000000\t0001\nenp6s0\t00000000\t0100A8C0\t0003\n";
        assert_eq!(default_interface(route).as_deref(), Some("enp6s0"));
        assert_eq!(default_interface("Iface\tDestination\n"), None);
    }

    #[test]
    fn clock_is_the_average_of_the_cores() {
        let cpuinfo = "processor\t: 0\ncpu MHz\t\t: 3000.000\n\nprocessor\t: 1\ncpu MHz\t\t: 4000.000\n";
        assert!((parse_cpu_ghz(cpuinfo).unwrap() - 3.5).abs() < 1e-4);
        assert_eq!(parse_cpu_ghz(""), None);
    }

    #[test]
    fn disks_and_gpu_from_their_tools() {
        let df = "Mounted on          1B-blocks         Used\n/boot           1071624192    141950976\n/home         817225457664 600546803712\n/             126268039168 105296515072\n";
        assert_eq!(
            parse_df(df),
            [
                Disk { mount: "/".into(), used: 105296515072, total: 126268039168 },
                Disk { mount: "/home".into(), used: 600546803712, total: 817225457664 },
                Disk { mount: "/boot".into(), used: 141950976, total: 1071624192 },
            ]
        );
        assert_eq!(
            parse_nvidia("39, 60, 1290, 8192\n"),
            Some(Gpu { percent: 39.0, temperature: 60.0, memory_used: 1290 << 20, memory_total: 8192 << 20 })
        );
        assert_eq!(parse_nvidia(""), None);
        assert_eq!(parse_nvidia("[N/A], 60, 1290, 8192"), None);
    }

    #[test]
    fn sizes_and_rates_read_like_the_design() {
        assert_eq!(format_gib(8_912_896_000), "8.3 GiB");
        assert_eq!(format_gib(0), "0.0 GiB");
        assert_eq!(format_rate(0.0), "0 B/s");
        assert_eq!(format_rate(512.0), "512 B/s");
        assert_eq!(format_rate(12_697.6), "12.4 KB/s");
        assert_eq!(format_rate(1_258_291.2), "1.2 MB/s");
    }

    #[test]
    fn readings_fill_the_usage_and_the_network_history() {
        let start = Instant::now();
        let mut usage = Usage { interface: Some("enp6s0".into()), ..Default::default() };
        usage.update(STAT, MEMINFO, NET_DEV, start);
        assert_eq!(usage.cpu, 0.0);
        assert!(usage.download.is_empty());

        let later = NET_DEV.replace("1000000 900", "1004096 950").replace("200000 500", "201024 520");
        usage.update(STAT_LATER, MEMINFO, &later, start + Duration::from_secs(2));
        assert!((usage.cpu - 44.444).abs() < 0.01);
        assert_eq!(usage.download, [2048.0]);
        assert_eq!(usage.upload, [512.0]);

        for step in 3..=(HISTORY as u64 + 5) {
            usage.update(STAT_LATER, MEMINFO, &later, start + Duration::from_secs(step));
        }
        assert_eq!(usage.download.len(), HISTORY);
    }
}
