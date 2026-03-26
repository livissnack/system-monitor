use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::{CpuExt, DiskExt, NetworkExt, System, SystemExt};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;
use warp::http::Response;
use warp::{Filter, Rejection};

// --- 1. 数据结构定义 ---

#[derive(Serialize, Deserialize, Clone)]
pub struct SystemInfo {
    pub system: SystemData,
    pub memory: MemoryInfo,
    pub cpu: CpuInfo,
    pub disk: DiskInfo,
    pub network: NetworkInfo,
    pub runtime: RuntimeInfo,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SystemData {
    pub hostname: String,
    pub os_name: String,
    pub kernel_version: String,
    pub uptime: u64,
    pub boot_time: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub usage: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    pub model: String,
    pub cores: usize,
    pub usage: f32,
    pub load_average: LoadAverage,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub usage: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkInfo {
    pub interfaces: Vec<NetworkInterface>,
    pub stats: NetworkStats,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub received: u64,
    pub transmitted: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkStats {
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub packets_received: u64,
    pub packets_sent: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RuntimeInfo {
    pub version: String,
    pub start_time: u64,
}

// --- 2. 监控逻辑实现 ---

pub struct SystemMonitor {
    sys: System,
    start_time: SystemTime,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys,
            start_time: SystemTime::now(),
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_memory();
        self.sys.refresh_cpu();
        self.sys.refresh_disks();
        self.sys.refresh_networks();
        self.sys.refresh_networks_list();
        self.sys.refresh_system();
    }

    pub fn get_system_info(&mut self) -> SystemInfo {
        self.refresh();
        SystemInfo {
            system: self.get_system_data(),
            memory: self.get_memory_info(),
            cpu: self.get_cpu_info(),
            disk: self.get_disk_info(),
            network: self.get_network_info(),
            runtime: self.get_runtime_info(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    fn get_system_data(&self) -> SystemData {
        SystemData {
            hostname: self
                .sys
                .host_name()
                .unwrap_or_else(|| "unknown".to_string()),
            os_name: self.sys.name().unwrap_or_else(|| "unknown".to_string()),
            kernel_version: self
                .sys
                .kernel_version()
                .unwrap_or_else(|| "unknown".to_string()),
            uptime: self.sys.uptime(),
            boot_time: self.sys.boot_time(),
        }
    }

    fn get_memory_info(&self) -> MemoryInfo {
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let free = self.sys.free_memory();
        let usage = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        MemoryInfo {
            total,
            used,
            free,
            usage,
        }
    }

    fn get_cpu_info(&self) -> CpuInfo {
        let load_avg = self.sys.load_average();
        let model = self
            .sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());
        CpuInfo {
            model,
            cores: self.sys.cpus().len(),
            usage: self.sys.global_cpu_info().cpu_usage(),
            load_average: LoadAverage {
                one: load_avg.one,
                five: load_avg.five,
                fifteen: load_avg.fifteen,
            },
        }
    }

    fn get_disk_info(&self) -> DiskInfo {
        let mut total = 0;
        let mut used = 0;
        let mut main_disk_name = String::from("Unknown");
        for disk in self.sys.disks() {
            total += disk.total_space();
            used += disk.total_space() - disk.available_space();
            if disk.mount_point() == std::path::Path::new("/") || main_disk_name == "Unknown" {
                main_disk_name = disk.name().to_string_lossy().into_owned();
            }
        }
        let free = total - used;
        let usage = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        DiskInfo {
            name: main_disk_name,
            total,
            used,
            free,
            usage,
        }
    }

    fn get_network_info(&self) -> NetworkInfo {
        let mut interfaces = Vec::new();
        let (mut total_received, mut total_sent) = (0, 0);
        let (mut total_packets_received, mut total_packets_sent) = (0, 0);

        for (interface_name, data) in self.sys.networks() {
            interfaces.push(NetworkInterface {
                name: interface_name.clone(),
                mac_address: data.mac_address().to_string(),
                received: data.total_received(),
                transmitted: data.total_transmitted(),
            });
            total_received += data.total_received();
            total_sent += data.total_transmitted();
            total_packets_received += data.total_packets_received();
            total_packets_sent += data.total_packets_transmitted();
        }

        NetworkInfo {
            interfaces,
            stats: NetworkStats {
                bytes_received: total_received,
                bytes_sent: total_sent,
                packets_received: total_packets_received,
                packets_sent: total_packets_sent,
            },
        }
    }

    fn get_runtime_info(&self) -> RuntimeInfo {
        RuntimeInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            start_time: self
                .start_time
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

// --- 3. Web 处理函数 ---

async fn sse_handler(interval_ms: u64) -> Result<impl warp::Reply, Infallible> {
    let stream = IntervalStream::new(interval(Duration::from_millis(interval_ms))).map(move |_| {
        let mut monitor = SystemMonitor::new();
        let info = monitor.get_system_info();
        let json = serde_json::to_string(&info).unwrap();
        Ok::<String, Infallible>(format!("data: {}\n\n", json))
    });

    let body = warp::hyper::Body::wrap_stream(stream);
    Ok(Response::builder()
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap())
}

async fn api_handler() -> Result<impl warp::Reply, Rejection> {
    let mut monitor = SystemMonitor::new();
    let info = monitor.get_system_info();
    Ok(warp::reply::json(&info))
}

// fn html_handler() -> impl warp::Reply {
//     let html = include_str!("index.html"); // 建议把 HTML 存为同目录下的 index.html
//     warp::reply::html(html)
// }

// HTML 页面 - 增加中英文切换功能
// HTML 页面 - 增加 Chart.js 实时折线图
fn html_handler() -> impl warp::Reply {
    let html = r#"
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>System Monitor Pro | Rust</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <style>
        @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;600;800&family=JetBrains+Mono:wght@400;600&display=swap');

        :root {
            --bg-color: #08080e;
            --card-bg: rgba(17, 18, 34, 0.9);
            --accent-color: #6366f1;
            --text-main: #f1f5f9;
            --text-dim: #64748b;
            --success: #10b981;
            --error: #f43f5e;
            --panel-border: rgba(255, 255, 255, 0.08);
        }

        html { color-scheme: dark; }
        * { margin: 0; padding: 0; box-sizing: border-box; }

        body {
            font-family: 'Inter', -apple-system, system-ui, sans-serif;
            background: var(--bg-color);
            background-image:
                radial-gradient(at 0% 0%, rgba(99, 102, 241, 0.18) 0px, transparent 50%),
                radial-gradient(at 100% 0%, rgba(16, 185, 129, 0.12) 0px, transparent 50%);
            color: var(--text-main);
            padding: 34px 18px;
            min-height: 100vh;
            line-height: 1.4;
        }

        .container { max-width: 1220px; margin: 0 auto; }

        .header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            gap: 16px;
            margin-bottom: 18px;
            padding: 16px 18px;
            border-radius: 18px;
            border: 1px solid var(--panel-border);
            background: rgba(17, 18, 34, 0.55);
            backdrop-filter: blur(10px);
        }

        .header h1 {
            font-size: 1.6rem;
            letter-spacing: 1px;
            font-weight: 800;
            background: linear-gradient(90deg, #ffffff 0%, rgba(99, 102, 241, 0.95) 55%, rgba(16, 185, 129, 0.9) 100%);
            -webkit-background-clip: text;
            background-clip: text;
            color: transparent;
            white-space: nowrap;
        }

        .lang-toggle {
            display: flex;
            gap: 4px;
            background: rgba(26, 27, 46, 0.7);
            border-radius: 20px;
            padding: 4px;
            border: 1px solid rgba(255, 255, 255, 0.06);
        }

        .lang-btn {
            padding: 6px 14px;
            font-size: 0.75rem;
            border-radius: 16px;
            cursor: pointer;
            transition: 0.2s ease;
            color: var(--text-dim);
            border: 1px solid transparent;
            user-select: none;
        }

        .lang-btn:hover { color: #fff; border-color: rgba(255, 255, 255, 0.08); }
        .lang-btn.active { background: var(--accent-color); color: white; border-color: rgba(99, 102, 241, 0.5); }

        .controls { display: flex; gap: 12px; margin-bottom: 16px; flex-wrap: wrap; }
        .control-group {
            background: rgba(17, 18, 34, 0.7);
            padding: 8px;
            border-radius: 14px;
            border: 1px solid var(--panel-border);
            display: flex;
            align-items: center;
            gap: 8px;
            backdrop-filter: blur(10px);
        }

        .control-group label {
            padding: 0 8px;
            font-size: 0.75rem;
            color: var(--text-dim);
            font-weight: 700;
            letter-spacing: 0.4px;
            white-space: nowrap;
        }

        button {
            padding: 7px 15px;
            background: rgba(255, 255, 255, 0.02);
            color: var(--text-dim);
            border: 1px solid rgba(255, 255, 255, 0.08);
            border-radius: 10px;
            cursor: pointer;
            font-size: 0.8rem;
            font-weight: 700;
            transition: 0.2s ease;
        }

        button:hover {
            color: #fff;
            background: rgba(99, 102, 241, 0.1);
            border-color: rgba(99, 102, 241, 0.45);
            transform: translateY(-1px);
        }

        button.active {
            background: rgba(99, 102, 241, 0.22);
            border-color: rgba(99, 102, 241, 0.7);
            color: #fff;
            box-shadow: 0 12px 35px rgba(99, 102, 241, 0.18);
        }

        #connectBtn {
            background: rgba(16, 185, 129, 0.08);
            color: var(--success);
            border-color: rgba(16, 185, 129, 0.35);
        }

        #connectBtn:hover {
            background: rgba(16, 185, 129, 0.14);
            border-color: rgba(16, 185, 129, 0.55);
        }

        #disconnectBtn {
            background: rgba(244, 63, 94, 0.08);
            color: var(--error);
            border-color: rgba(244, 63, 94, 0.25);
        }

        #disconnectBtn:hover {
            background: rgba(244, 63, 94, 0.14);
            border-color: rgba(244, 63, 94, 0.45);
        }

        button:disabled {
            opacity: 0.35;
            cursor: not-allowed;
            transform: none;
        }

        .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(360px, 1fr)); gap: 16px; }

        .card {
            background: var(--card-bg);
            border-radius: 16px;
            border: 1px solid var(--panel-border);
            padding: 18px 18px 16px;
            display: flex;
            flex-direction: column;
            min-height: 214px;
            box-shadow: 0 18px 50px rgba(0, 0, 0, 0.25);
            transition: transform 0.2s ease, border-color 0.2s ease;
        }

        .card:hover { transform: translateY(-2px); border-color: rgba(99, 102, 241, 0.3); }

        .card h3 {
            margin-bottom: 18px;
            font-size: 0.8rem;
            color: var(--text-dim);
            text-transform: uppercase;
            letter-spacing: 1.5px;
            display: flex;
            align-items: center;
        }

        .card h3::before {
            content: '';
            width: 4px;
            height: 14px;
            background: var(--accent-color);
            margin-right: 10px;
            border-radius: 2px;
        }

        .chart-container {
            height: 120px;
            margin-top: auto;
            padding: 12px;
            border-radius: 14px;
            background: rgba(255, 255, 255, 0.03);
            border: 1px solid rgba(255, 255, 255, 0.05);
        }

        .chart-container canvas { width: 100% !important; height: 100% !important; }

        .stat-item { display: flex; justify-content: space-between; margin-bottom: 10px; }
        .stat-label { color: var(--text-dim); font-size: 0.8rem; }
        .stat-value { font-family: 'JetBrains Mono', monospace; font-size: 0.9rem; color: #fff; }

        .progress-bar {
            background: rgba(255, 255, 255, 0.05);
            height: 8px;
            border-radius: 999px;
            overflow: hidden;
            margin: 12px 0 14px;
        }

        .progress-fill {
            height: 100%;
            background: linear-gradient(90deg, rgba(99, 102, 241, 1) 0%, rgba(99, 102, 241, 0.7) 100%);
            width: 0%;
            transition: 0.5s;
            border-radius: 999px;
        }

        .full-width { grid-column: 1 / -1; }
        .net-group { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }

        .footer {
            display: flex;
            justify-content: space-between;
            margin-top: 18px;
            font-size: 0.75rem;
            color: var(--text-dim);
            opacity: 0.9;
        }

        .error {
            color: var(--error);
            background: rgba(244, 63, 94, 0.1);
            padding: 10px;
            border-radius: 10px;
            margin-bottom: 15px;
            border: 1px solid rgba(244, 63, 94, 0.2);
        }

        @media (max-width: 900px) {
            .header h1 { white-space: normal; }
            .stats-grid { grid-template-columns: 1fr; }
            .net-group { grid-template-columns: 1fr; gap: 14px; }
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1 id="i18n-title">SYSTEM DASHBOARD</h1>
            <div class="lang-toggle">
                <div class="lang-btn active" id="btn-en" onclick="setLang('en')">EN</div>
                <div class="lang-btn" id="btn-zh" onclick="setLang('zh')">中文</div>
            </div>
        </div>

        <div class="controls">
            <div class="control-group">
                <label id="i18n-refresh">REFRESH</label>
                <button onclick="changeInterval(1000, this)">1s</button>
                <button onclick="changeInterval(2000, this)" class="active">2s</button>
                <button onclick="changeInterval(5000, this)">5s</button>
            </div>
            <div class="control-group">
                <button onclick="connect()" id="connectBtn">CONNECT</button>
                <button onclick="disconnect()" id="disconnectBtn" disabled>STOP</button>
            </div>
        </div>

        <div id="errorContainer" style="display: none;"></div>

        <div class="stats-grid">
            <div class="card">
                <h3 id="i18n-cpu-title">CPU USAGE</h3>
                <div class="stat-item"><span class="stat-label" id="i18n-cpu-load">LOAD</span><span class="stat-value" id="cpuLoad">-</span></div>
                <div class="stat-item"><span class="stat-label" id="i18n-cpu-usage">USAGE</span><span class="stat-value" id="cpuUsage">-</span></div>
                <div class="progress-bar"><div class="progress-fill" id="cpuProgress"></div></div>
                <div class="chart-container"><canvas id="cpuChart"></canvas></div>
            </div>

            <div class="card">
                <h3 id="i18n-mem-title">MEMORY</h3>
                <div class="stat-item"><span class="stat-label" id="i18n-mem-total">TOTAL</span><span class="stat-value" id="memoryTotal">-</span></div>
                <div class="stat-item"><span class="stat-label" id="i18n-mem-usage">USAGE</span><span class="stat-value" id="memoryUsage">-</span></div>
                <div class="progress-bar"><div class="progress-fill" id="memoryProgress"></div></div>
                <div class="chart-container"><canvas id="memChart"></canvas></div>
            </div>

            <div class="card">
                <h3 id="i18n-os-title">HOST INFO</h3>
                <div class="stat-item"><span class="stat-label" id="i18n-host">HOSTNAME</span><span class="stat-value" id="hostname">-</span></div>
                <div class="stat-item"><span class="stat-label" id="i18n-os">OS</span><span class="stat-value" id="osName">-</span></div>
                <div class="stat-item"><span class="stat-label" id="i18n-uptime">UPTIME</span><span class="stat-value" id="systemUptime">-</span></div>
                <div style="margin-top:auto; font-size: 0.7rem; color: var(--text-dim);" id="kernelDisplay">Kernel: -</div>
            </div>

            <div class="card full-width">
                <h3 id="i18n-net-title">NETWORK</h3>
                <div class="net-group">
                    <div>
                        <div class="stat-item"><span class="stat-label" id="i18n-net-sent">SENT</span><span class="stat-value" id="bytesSent">-</span></div>
                        <div class="stat-item"><span class="stat-label" id="i18n-net-psent">PKTS OUT</span><span class="stat-value" id="packetsSent">-</span></div>
                    </div>
                    <div>
                        <div class="stat-item"><span class="stat-label" id="i18n-net-recv">RECV</span><span class="stat-value" id="bytesReceived">-</span></div>
                        <div class="stat-item"><span class="stat-label" id="i18n-net-precv">PKTS IN</span><span class="stat-value" id="packetsReceived">-</span></div>
                    </div>
                </div>
            </div>
        </div>

        <div class="footer">
            <span id="timestamp">Waiting...</span>
            <span>Powered by Rust & Warp</span>
        </div>
    </div>

    <script>
        let eventSource = null;
        let currentInterval = 2000;
        let currentLang = 'en';
        let cpuChart, memChart;

        const i18n = {
            en: {
                title: "SYSTEM DASHBOARD", refresh: "REFRESH", connect: "CONNECT", stop: "STOP",
                os_title: "HOST INFO", host: "HOSTNAME", os: "OS", uptime: "UPTIME",
                mem_title: "MEMORY", mem_total: "TOTAL", mem_usage: "USAGE",
                cpu_title: "CPU USAGE", cpu_load: "LOAD AVG", cpu_usage: "USAGE",
                net_title: "NETWORK", net_sent: "SENT", net_psent: "PKTS OUT", net_recv: "RECV", net_precv: "PKTS IN",
                last_update: "Last Update", err_token: "Access Token Required"
            },
            zh: {
                title: "系统监控中心", refresh: "刷新间隔", connect: "建立连接", stop: "停止",
                os_title: "主机信息", host: "主机名称", os: "操作系统", uptime: "运行时间",
                mem_title: "内存状态", mem_total: "总内存容量", mem_usage: "当前利用率",
                cpu_title: "处理器负载", cpu_load: "平均负载", cpu_usage: "使用率",
                net_title: "网络实时流量", net_sent: "已发送", net_psent: "发送包数", net_recv: "已接收", net_precv: "接收包数",
                last_update: "数据最后更新", err_token: "URL 缺失访问令牌"
            }
        };

        function initCharts() {
            const commonOptions = {
                responsive: true, maintainAspectRatio: false,
                scales: {
                    y: { min: 0, max: 100, display: false },
                    x: { display: false }
                },
                plugins: { legend: { display: false } },
                elements: { line: { tension: 0.4 }, point: { radius: 0 } },
                animation: { duration: 400 }
            };

            const ctxCpu = document.getElementById('cpuChart').getContext('2d');
            cpuChart = new Chart(ctxCpu, {
                type: 'line',
                data: { labels: Array(30).fill(''), datasets: [{ data: Array(30).fill(0), borderColor: '#6366f1', fill: true, backgroundColor: 'rgba(99, 102, 241, 0.1)' }] },
                options: commonOptions
            });

            const ctxMem = document.getElementById('memChart').getContext('2d');
            memChart = new Chart(ctxMem, {
                type: 'line',
                data: { labels: Array(30).fill(''), datasets: [{ data: Array(30).fill(0), borderColor: '#10b981', fill: true, backgroundColor: 'rgba(16, 185, 129, 0.1)' }] },
                options: commonOptions
            });
        }

        function setLang(lang) {
            currentLang = lang;
            document.getElementById('btn-en').classList.toggle('active', lang === 'en');
            document.getElementById('btn-zh').classList.toggle('active', lang === 'zh');
            const t = i18n[lang];
            for (let id in t) {
                const el = document.getElementById('i18n-' + id.replace('_', '-'));
                if (el) el.innerText = t[id];
            }
        }

        function updateDisplay(data) {
            // Update Text
            document.getElementById('hostname').textContent = data.system.hostname;
            document.getElementById('osName').textContent = data.system.os_name;
            document.getElementById('systemUptime').textContent = formatUptime(data.system.uptime);
            document.getElementById('kernelDisplay').textContent = `Kernel: ${data.system.kernel_version}`;

            document.getElementById('memoryTotal').textContent = formatBytes(data.memory.total);
            document.getElementById('memoryUsage').textContent = `${data.memory.usage.toFixed(1)}%`;
            document.getElementById('memoryProgress').style.width = `${data.memory.usage}%`;

            document.getElementById('cpuUsage').textContent = `${data.cpu.usage.toFixed(1)}%`;
            document.getElementById('cpuLoad').textContent = `${data.cpu.load_average.one.toFixed(2)} / ${data.cpu.load_average.five.toFixed(2)}`;
            document.getElementById('cpuProgress').style.width = `${data.cpu.usage}%`;

            document.getElementById('bytesSent').textContent = formatBytes(data.network.stats.bytes_sent);
            document.getElementById('bytesReceived').textContent = formatBytes(data.network.stats.bytes_received);
            document.getElementById('packetsSent').textContent = data.network.stats.packets_sent.toLocaleString();
            document.getElementById('packetsReceived').textContent = data.network.stats.packets_received.toLocaleString();
            document.getElementById('timestamp').textContent = `${i18n[currentLang].last_update}: ${new Date(data.timestamp * 1000).toLocaleTimeString()}`;

            // Update Charts
            updateChart(cpuChart, data.cpu.usage);
            updateChart(memChart, data.memory.usage);
        }

        function updateChart(chart, val) {
            chart.data.datasets[0].data.push(val);
            chart.data.datasets[0].data.shift();
            chart.update('none');
        }

        function connect() {
            const token = new URLSearchParams(window.location.search).get('token');
            if (!token) { showError(i18n[currentLang].err_token); return; }
            showError('');
            eventSource = new EventSource(`/sse?token=${token}&interval=${currentInterval}`);
            eventSource.onopen = () => {
                document.getElementById('connectBtn').disabled = true;
                document.getElementById('disconnectBtn').disabled = false;
            };
            eventSource.onmessage = (e) => updateDisplay(JSON.parse(e.data));
            eventSource.onerror = () => { disconnect(); setTimeout(connect, 3000); };
        }

        function disconnect() { if (eventSource) { eventSource.close(); eventSource = null; }
            document.getElementById('connectBtn').disabled = false;
            document.getElementById('disconnectBtn').disabled = true;
        }

        function changeInterval(ms, btn) {
            currentInterval = ms;
            btn.parentElement.querySelectorAll('button').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            if (eventSource) { disconnect(); connect(); }
        }

        function formatBytes(b) {
            if (b === 0) return "0 B";
            const s = ['B', 'KB', 'MB', 'GB', 'TB'];
            const i = Math.floor(Math.log(b) / Math.log(1024));
            return (b / Math.pow(1024, i)).toFixed(2) + ' ' + s[i];
        }

        function formatUptime(s) {
            const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
            return currentLang === 'zh' ? `${d}天${h}时${m}分` : `${d}d ${h}h ${m}m`;
        }

        function showError(m) {
            const c = document.getElementById('errorContainer');
            c.innerHTML = m ? `<div class="error">${m}</div>` : '';
            c.style.display = m ? 'block' : 'none';
        }

        window.onload = () => { initCharts(); setLang('zh'); connect(); };
    </script>
</body>
</html>
    "#;
    warp::reply::html(html)
}

// --- 4. 认证与错误处理 ---

#[derive(Debug)]
struct AuthFailure;
impl warp::reject::Reject for AuthFailure {}

async fn handle_rejection(err: Rejection) -> Result<impl warp::Reply, Infallible> {
    if err.find::<AuthFailure>().is_some() {
        Ok(warp::reply::with_status(
            "Invalid Token",
            warp::http::StatusCode::FORBIDDEN,
        ))
    } else {
        Ok(warp::reply::with_status(
            "Internal Error",
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}

// --- 5. 主函数 ---

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8080);
    let auth_token = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "m1MLaC23MmDNmkPf21STPnNDPSv9P7".to_string());

    println!("🚀 Server: http://0.0.0.0:{}", port);
    println!("🔐 Token: {}", auth_token);
    println!("  GET /sse?token=m1MLaC23MmDNmkPf21STPnNDPSv9P7     - SSE 流式系统监控");
    println!("  GET /api?token=m1MLaC23MmDNmkPf21STPnNDPSv9P7     - 一次性系统信息 (JSON)");
    println!("  GET /?token=m1MLaC23MmDNmkPf21STPnNDPSv9P7       - 网页监控界面\n");

    let token_filter = warp::any().map(move || auth_token.clone());

    let with_auth = warp::query::<HashMap<String, String>>()
        .and(token_filter)
        .and_then(
            |params: HashMap<String, String>, required: String| async move {
                if params.get("token") == Some(&required) {
                    Ok(())
                } else {
                    Err(warp::reject::custom(AuthFailure))
                }
            },
        );

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET"])
        .allow_headers(vec!["Content-Type"]);

    // SSE 路由：注意闭包多了一个 _ 参数来接收 with_auth 传来的 ()
    let sse_route = warp::path("sse")
        .and(warp::get())
        .and(with_auth.clone())
        .and(warp::query::<HashMap<String, String>>())
        .and_then(|_auth, params: HashMap<String, String>| async move {
            let interval_ms = params
                .get("interval")
                .and_then(|s| s.parse().ok())
                .unwrap_or(2000);
            sse_handler(interval_ms).await
        });

    let api_route = warp::path("api")
        .and(warp::get())
        .and(with_auth.clone())
        .and_then(|_auth| api_handler());

    let html_route = warp::path::end()
        .and(warp::get())
        .and(with_auth.clone())
        .map(|_auth| html_handler());

    let routes = sse_route
        .or(api_route)
        .or(html_route)
        .with(cors)
        .recover(handle_rejection);

    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
}
