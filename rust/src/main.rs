use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use sysinfo::{System, SystemExt, CpuExt, DiskExt, NetworkExt};
use warp::{Filter, Rejection};
use std::convert::Infallible;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;
use warp::http::Response;

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
            hostname: self.sys.host_name().unwrap_or_else(|| "unknown".to_string()),
            os_name: self.sys.name().unwrap_or_else(|| "unknown".to_string()),
            kernel_version: self.sys.kernel_version().unwrap_or_else(|| "unknown".to_string()),
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
        
        CpuInfo {
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

        for disk in self.sys.disks() {
            total += disk.total_space();
            used += disk.total_space() - disk.available_space();
        }

        let free = total - used;
        let usage = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        DiskInfo {
            total,
            used,
            free,
            usage,
        }
    }

    fn get_network_info(&self) -> NetworkInfo {
        let mut interfaces = Vec::new();
        let mut total_received = 0;
        let mut total_sent = 0;
        let mut total_packets_received = 0;
        let mut total_packets_sent = 0;

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
            start_time: self.start_time
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

// SSE 事件流
async fn sse_handler(
    interval_ms: u64,
) -> Result<impl warp::Reply, Infallible> {
    let stream = IntervalStream::new(interval(Duration::from_millis(interval_ms)))
        .map(move |_| {
            let mut monitor = SystemMonitor::new();
            let info = monitor.get_system_info();
            let json = serde_json::to_string(&info).unwrap();
            Ok::<String, Infallible>(format!("data: {}\n\n", json))
        });

    let body = warp::hyper::Body::wrap_stream(stream);

    let response = Response::builder()
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Headers", "Cache-Control")
        .header("Transfer-Encoding", "chunked")
        .body(body)
        .unwrap();

    Ok(response)
}

// API 端点
async fn api_handler() -> Result<impl warp::Reply, Rejection> {
    let mut monitor = SystemMonitor::new();
    let info = monitor.get_system_info();
    Ok(warp::reply::json(&info))
}

// HTML 页面
fn html_handler() -> impl warp::Reply {
    let html = r#"
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>系统监控面板</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #0f0f23; color: #fff; padding: 20px; }
        .container { max-width: 1200px; margin: 0 auto; }
        .header { text-align: center; margin-bottom: 30px; padding: 20px; background: #1a1a2e; border-radius: 10px; }
        .controls { display: flex; gap: 15px; margin-bottom: 20px; flex-wrap: wrap; }
        .control-group { background: #1a1a2e; padding: 15px; border-radius: 8px; }
        button { padding: 10px 20px; background: #4a4a8a; color: white; border: none; border-radius: 5px; cursor: pointer; }
        button:hover { background: #5a5a9a; }
        button.active { background: #6a6aaa; }
        .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; margin-bottom: 20px; }
        .card { background: #1a1a2e; padding: 20px; border-radius: 10px; border-left: 4px solid #4a4a8a; }
        .card h3 { margin-bottom: 15px; color: #8a8aff; }
        .stat-item { display: flex; justify-content: space-between; margin-bottom: 8px; }
        .stat-label { color: #aaa; }
        .stat-value { font-weight: bold; }
        .progress-bar { background: #2a2a3e; height: 20px; border-radius: 10px; margin: 10px 0; overflow: hidden; }
        .progress-fill { height: 100%; background: linear-gradient(90deg, #4a4a8a, #8a8aff); transition: width 0.3s ease; }
        .network-stats { display: grid; grid-template-columns: 1fr 1fr; gap: 15px; }
        .timestamp { text-align: center; color: #888; margin-top: 20px; }
        .error { color: #ff6b6b; background: #2a1a1a; padding: 10px; border-radius: 5px; margin: 10px 0; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🖥️ 实时系统监控面板 (Rust)</h1>
            <p>通过 Server-Sent Events (SSE) 实时监控系统状态</p>
        </div>

        <div class="controls">
            <div class="control-group">
                <label>刷新间隔: </label>
                <button onclick="setInterval(1000)">1秒</button>
                <button onclick="setInterval(2000)" class="active">2秒</button>
                <button onclick="setInterval(5000)">5秒</button>
            </div>
            <div class="control-group">
                <button onclick="connect()" id="connectBtn">🔗 连接监控</button>
                <button onclick="disconnect()" id="disconnectBtn">❌ 断开连接</button>
            </div>
        </div>

        <div id="errorContainer" style="display: none;"></div>

        <div class="stats-grid" id="statsGrid">
            <div class="card">
                <h3>🖥️ 系统信息</h3>
                <div class="stat-item"><span class="stat-label">主机名:</span><span class="stat-value" id="hostname">-</span></div>
                <div class="stat-item"><span class="stat-label">系统:</span><span class="stat-value" id="osName">-</span></div>
                <div class="stat-item"><span class="stat-label">内核:</span><span class="stat-value" id="kernelVersion">-</span></div>
                <div class="stat-item"><span class="stat-label">运行时间:</span><span class="stat-value" id="systemUptime">-</span></div>
            </div>

            <div class="card">
                <h3>🧠 内存使用</h3>
                <div class="stat-item"><span class="stat-label">总内存:</span><span class="stat-value" id="memoryTotal">-</span></div>
                <div class="stat-item"><span class="stat-label">已使用:</span><span class="stat-value" id="memoryUsed">-</span></div>
                <div class="stat-item"><span class="stat-label">使用率:</span><span class="stat-value" id="memoryUsage">-</span></div>
                <div class="progress-bar"><div class="progress-fill" id="memoryProgress" style="width: 0%"></div></div>
            </div>

            <div class="card">
                <h3>⚡ CPU 信息</h3>
                <div class="stat-item"><span class="stat-label">核心数:</span><span class="stat-value" id="cpuCores">-</span></div>
                <div class="stat-item"><span class="stat-label">使用率:</span><span class="stat-value" id="cpuUsage">-</span></div>
                <div class="stat-item"><span class="stat-label">负载 (1/5/15分钟):</span><span class="stat-value" id="cpuLoad">-</span></div>
                <div class="progress-bar"><div class="progress-fill" id="cpuProgress" style="width: 0%"></div></div>
            </div>

            <div class="card">
                <h3>💾 磁盘信息</h3>
                <div class="stat-item"><span class="stat-label">总空间:</span><span class="stat-value" id="diskTotal">-</span></div>
                <div class="stat-item"><span class="stat-label">已使用:</span><span class="stat-value" id="diskUsed">-</span></div>
                <div class="stat-item"><span class="stat-label">使用率:</span><span class="stat-value" id="diskUsage">-</span></div>
                <div class="progress-bar"><div class="progress-fill" id="diskProgress" style="width: 0%"></div></div>
            </div>

            <div class="card" style="grid-column: span 2;">
                <h3>🌐 网络统计</h3>
                <div class="network-stats">
                    <div>
                        <div class="stat-item"><span class="stat-label">总上传:</span><span class="stat-value" id="bytesSent">-</span></div>
                        <div class="stat-item"><span class="stat-label">上传包数:</span><span class="stat-value" id="packetsSent">-</span></div>
                    </div>
                    <div>
                        <div class="stat-item"><span class="stat-label">总下载:</span><span class="stat-value" id="bytesReceived">-</span></div>
                        <div class="stat-item"><span class="stat-label">下载包数:</span><span class="stat-value" id="packetsReceived">-</span></div>
                    </div>
                </div>
            </div>
        </div>

        <div class="timestamp" id="timestamp">最后更新: -</div>
    </div>

    <script>
        let eventSource = null;
        let currentInterval = 2000;

        function setInterval(interval) {
            currentInterval = interval;
            document.querySelectorAll('.control-group button').forEach(btn => btn.classList.remove('active'));
            event.target.classList.add('active');
            if (eventSource) {
                disconnect();
                connect();
            }
        }

        function connect() {
            const params = new URLSearchParams({
                interval: currentInterval
            });

            eventSource = new EventSource('/sse?' + params.toString());
            
            eventSource.onopen = () => {
                console.log('SSE 连接已建立');
                showError('');
            };

            eventSource.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    updateDisplay(data);
                } catch (e) {
                    console.error('解析数据失败:', e);
                }
            };

            eventSource.onerror = (error) => {
                console.error('SSE 连接错误:', error);
                showError('连接错误，尝试重连中...');
                setTimeout(() => {
                    disconnect();
                    connect();
                }, 5000);
            };

            document.getElementById('connectBtn').disabled = true;
            document.getElementById('disconnectBtn').disabled = false;
        }

        function disconnect() {
            if (eventSource) {
                eventSource.close();
                eventSource = null;
            }
            document.getElementById('connectBtn').disabled = false;
            document.getElementById('disconnectBtn').disabled = true;
        }

        function updateDisplay(data) {
            // 系统信息
            document.getElementById('hostname').textContent = data.system.hostname;
            document.getElementById('osName').textContent = data.system.os_name;
            document.getElementById('kernelVersion').textContent = data.system.kernel_version;
            document.getElementById('systemUptime').textContent = formatUptime(data.system.uptime);

            // 内存信息
            document.getElementById('memoryTotal').textContent = formatBytes(data.memory.total);
            document.getElementById('memoryUsed').textContent = formatBytes(data.memory.used);
            document.getElementById('memoryUsage').textContent = `${data.memory.usage.toFixed(1)}%`;
            document.getElementById('memoryProgress').style.width = `${data.memory.usage}%`;

            // CPU 信息
            document.getElementById('cpuCores').textContent = data.cpu.cores;
            document.getElementById('cpuUsage').textContent = `${data.cpu.usage.toFixed(1)}%`;
            document.getElementById('cpuLoad').textContent = `${data.cpu.load_average.one.toFixed(2)} / ${data.cpu.load_average.five.toFixed(2)} / ${data.cpu.load_average.fifteen.toFixed(2)}`;
            document.getElementById('cpuProgress').style.width = `${data.cpu.usage}%`;

            // 磁盘信息
            document.getElementById('diskTotal').textContent = formatBytes(data.disk.total);
            document.getElementById('diskUsed').textContent = formatBytes(data.disk.used);
            document.getElementById('diskUsage').textContent = `${data.disk.usage.toFixed(1)}%`;
            document.getElementById('diskProgress').style.width = `${data.disk.usage}%`;

            // 网络信息
            document.getElementById('bytesSent').textContent = formatBytes(data.network.stats.bytes_sent);
            document.getElementById('bytesReceived').textContent = formatBytes(data.network.stats.bytes_received);
            document.getElementById('packetsSent').textContent = data.network.stats.packets_sent.toLocaleString();
            document.getElementById('packetsReceived').textContent = data.network.stats.packets_received.toLocaleString();

            // 时间戳
            document.getElementById('timestamp').textContent = `最后更新: ${new Date(data.timestamp * 1000).toLocaleString()}`;
        }

        function showError(message) {
            const container = document.getElementById('errorContainer');
            if (message) {
                container.innerHTML = `<div class="error">${message}</div>`;
                container.style.display = 'block';
            } else {
                container.style.display = 'none';
            }
        }

        function formatUptime(seconds) {
            const days = Math.floor(seconds / 86400);
            const hours = Math.floor((seconds % 86400) / 3600);
            const minutes = Math.floor((seconds % 3600) / 60);
            const secs = seconds % 60;

            const parts = [];
            if (days > 0) parts.push(`${days}天`);
            if (hours > 0) parts.push(`${hours}小时`);
            if (minutes > 0) parts.push(`${minutes}分`);
            parts.push(`${secs}秒`);

            return parts.join('');
        }

        function formatBytes(bytes) {
            if (bytes === 0) return "0 B";
            const units = ['B', 'KB', 'MB', 'GB', 'TB'];
            let size = bytes;
            let unitIndex = 0;
            while (size >= 1024 && unitIndex < units.length - 1) {
                size /= 1024;
                unitIndex++;
            }
            return `${size.toFixed(2)} ${units[unitIndex]}`;
        }

        // 页面加载完成后自动连接
        window.addEventListener('load', connect);
    </script>
</body>
</html>
    "#;

    warp::reply::html(html)
}

#[tokio::main]
async fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    println!("🚀 启动系统监控服务器 http://0.0.0.0:{}", port);
    println!("可用端点:");
    println!("  GET /sse     - SSE 流式系统监控");
    println!("  GET /api     - 一次性系统信息 (JSON)");
    println!("  GET /        - 网页监控界面\n");

    // CORS 中间件
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["Content-Type"]);

    // 路由定义 - 最简单的版本
    let sse_route = warp::path("sse")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and_then(|params: HashMap<String, String>| async move {
            let interval_ms = params
                .get("interval")
                .and_then(|s| s.parse().ok())
                .unwrap_or(2000);
            sse_handler(interval_ms).await
        });

    let api_route = warp::path("api")
        .and(warp::get())
        .and_then(api_handler);

    let html_route = warp::path::end()
        .and(warp::get())
        .map(html_handler);

    let routes = sse_route
        .or(api_route)
        .or(html_route)
        .with(cors);

    warp::serve(routes)
        .run(([0, 0, 0, 0], port))
        .await;
}