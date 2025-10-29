// main.ts
import { SystemMonitorSSE, formatBytes, formatUptime } from "./system.ts";

// HTTP 服务器类
class SystemMonitorServer {
  private port: number;

  constructor(port: number = 8080) {
    this.port = port;
  }

  async start() {
    console.log(`🚀 启动系统监控服务器 http://0.0.0.0:${this.port}`);
    console.log("可用端点:");
    console.log(`  GET /sse     - SSE 流式系统监控`);
    console.log(`  GET /api     - 一次性系统信息 (JSON)`);
    console.log(`  GET /        - 网页监控界面\n`);

    const handler = async (request: Request): Promise<Response> => {
      const url = new URL(request.url);
      
      switch (url.pathname) {
        case "/sse":
          return this.handleSSE(request);
        case "/api":
          return await this.handleAPI();
        case "/":
          return this.handleHTML();
        default:
          return new Response("Not Found", { status: 404 });
      }
    };

    Deno.serve({ port: this.port }, handler);
  }

  // 处理 SSE 请求
  private handleSSE(request: Request): Response {
    // 解析查询参数
    const url = new URL(request.url);
    const interval = parseInt(url.searchParams.get("interval") || "2000");
    const enableCpuUsage = url.searchParams.get("cpu") !== "false";
    const enableNetworkStats = url.searchParams.get("network") !== "false";

    console.log(`📡 新的 SSE 连接: interval=${interval}ms, cpu=${enableCpuUsage}, network=${enableNetworkStats}`);

    const monitor = new SystemMonitorSSE({
      interval,
      enableCpuUsage,
      enableNetworkStats
    });

    const stream = monitor.createStream();

    return new Response(stream, {
      headers: {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        "Connection": "keep-alive",
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Headers": "Cache-Control"
      }
    });
  }

  // 处理 API 请求
  private async handleAPI(): Promise<Response> {
    const { getSystemInfo } = await import("./system.ts");
    
    try {
      const systemInfo = await getSystemInfo({
        enableCpuUsage: true,
        enableNetworkStats: true
      });

      return new Response(JSON.stringify(systemInfo, null, 2), {
        headers: {
          "Content-Type": "application/json",
          "Access-Control-Allow-Origin": "*"
        }
      });
    } catch (error) {
      return new Response(JSON.stringify({
        error: error instanceof Error ? error.message : String(error)
      }), {
        status: 500,
        headers: {
          "Content-Type": "application/json",
          "Access-Control-Allow-Origin": "*"
        }
      });
    }
  }

  // 提供网页界面
  private handleHTML(): Response {
    const html = `
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
        .stat-item { display: flex; justify-content: between; margin-bottom: 8px; }
        .stat-label { flex: 1; color: #aaa; }
        .stat-value { flex: 1; text-align: right; font-weight: bold; }
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
            <h1>🖥️ 实时系统监控面板</h1>
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
                <label>监控选项: </label>
                <button onclick="toggleCpu()" class="active" id="cpuBtn">CPU监控: 开启</button>
                <button onclick="toggleNetwork()" class="active" id="networkBtn">网络监控: 开启</button>
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
                <div class="stat-item"><span class="stat-label">平台:</span><span class="stat-value" id="platform">-</span></div>
                <div class="stat-item"><span class="stat-label">运行时间:</span><span class="stat-value" id="systemUptime">-</span></div>
                <div class="stat-item"><span class="stat-label">进程运行时间:</span><span class="stat-value" id="processUptime">-</span></div>
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
                        <div class="stat-item"><span class="stat-label">上传速度:</span><span class="stat-value" id="uploadSpeed">-</span></div>
                        <div class="stat-item"><span class="stat-label">总上传:</span><span class="stat-value" id="bytesSent">-</span></div>
                        <div class="stat-item"><span class="stat-label">上传包数:</span><span class="stat-value" id="packetsSent">-</span></div>
                    </div>
                    <div>
                        <div class="stat-item"><span class="stat-label">下载速度:</span><span class="stat-value" id="downloadSpeed">-</span></div>
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
        let enableCpu = true;
        let enableNetwork = true;

        function setInterval(interval) {
            currentInterval = interval;
            document.querySelectorAll('.control-group button').forEach(btn => btn.classList.remove('active'));
            event.target.classList.add('active');
            if (eventSource) {
                disconnect();
                connect();
            }
        }

        function toggleCpu() {
            enableCpu = !enableCpu;
            const btn = document.getElementById('cpuBtn');
            btn.textContent = 'CPU监控: ' + (enableCpu ? '开启' : '关闭');
            btn.classList.toggle('active', enableCpu);
            if (eventSource) {
                disconnect();
                connect();
            }
        }

        function toggleNetwork() {
            enableNetwork = !enableNetwork;
            const btn = document.getElementById('networkBtn');
            btn.textContent = '网络监控: ' + (enableNetwork ? '开启' : '关闭');
            btn.classList.toggle('active', enableNetwork);
            if (eventSource) {
                disconnect();
                connect();
            }
        }

        function connect() {
            const params = new URLSearchParams({
                interval: currentInterval,
                cpu: enableCpu,
                network: enableNetwork
            });

            eventSource = new EventSource('/sse?' + params.toString());
            
            eventSource.onopen = () => {
                console.log('SSE 连接已建立');
                showError('');
            };

            eventSource.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    if (data.type === 'system_info') {
                        updateDisplay(data.data);
                    } else if (data.type === 'error') {
                        showError('监控错误: ' + data.error);
                    }
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
            document.getElementById('platform').textContent = \`\${data.system.platform} (\${data.system.arch})\`;
            document.getElementById('systemUptime').textContent = formatUptime(data.system.uptime);
            document.getElementById('processUptime').textContent = formatUptime(data.process.uptime);

            // 内存信息
            document.getElementById('memoryTotal').textContent = \`\${data.memory.total} MB\`;
            document.getElementById('memoryUsed').textContent = \`\${data.memory.used} MB\`;
            document.getElementById('memoryUsage').textContent = \`\${data.memory.usage}%\`;
            document.getElementById('memoryProgress').style.width = \`\${data.memory.usage}%\`;

            // CPU 信息
            document.getElementById('cpuCores').textContent = data.cpu.cores;
            document.getElementById('cpuUsage').textContent = \`\${data.cpu.usage.toFixed(1)}%\`;
            document.getElementById('cpuLoad').textContent = \`\${data.cpu.loadAverage.map(v => v.toFixed(2)).join(' / ')}\`;
            document.getElementById('cpuProgress').style.width = \`\${data.cpu.usage}%\`;

            // 磁盘信息
            document.getElementById('diskTotal').textContent = \`\${data.disk.total} GB\`;
            document.getElementById('diskUsed').textContent = \`\${data.disk.used} GB\`;
            document.getElementById('diskUsage').textContent = \`\${data.disk.usage}%\`;
            document.getElementById('diskProgress').style.width = \`\${data.disk.usage}%\`;

            // 网络信息
            document.getElementById('uploadSpeed').textContent = formatSpeed(data.network.stats.uploadSpeed);
            document.getElementById('downloadSpeed').textContent = formatSpeed(data.network.stats.downloadSpeed);
            document.getElementById('bytesSent').textContent = formatBytes(data.network.stats.bytesSent);
            document.getElementById('bytesReceived').textContent = formatBytes(data.network.stats.bytesReceived);
            document.getElementById('packetsSent').textContent = data.network.stats.packetsSent.toLocaleString();
            document.getElementById('packetsReceived').textContent = data.network.stats.packetsReceived.toLocaleString();

            // 时间戳
            document.getElementById('timestamp').textContent = \`最后更新: \${new Date(data.timestamp).toLocaleString()}\`;
        }

        function showError(message) {
            const container = document.getElementById('errorContainer');
            if (message) {
                container.innerHTML = \`<div class="error">\${message}</div>\`;
                container.style.display = 'block';
            } else {
                container.style.display = 'none';
            }
        }

        function formatUptime(milliseconds) {
            const seconds = Math.floor(milliseconds / 1000);
            const days = Math.floor(seconds / 86400);
            const hours = Math.floor((seconds % 86400) / 3600);
            const minutes = Math.floor((seconds % 3600) / 60);
            const secs = seconds % 60;

            const parts = [];
            if (days > 0) parts.push(\`\${days}天\`);
            if (hours > 0) parts.push(\`\${hours}小时\`);
            if (minutes > 0) parts.push(\`\${minutes}分\`);
            parts.push(\`\${secs}秒\`);

            return parts.join(' ');
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
            return \`\${size.toFixed(2)} \${units[unitIndex]}\`;
        }

        function formatSpeed(bytesPerSecond) {
            return \`\${formatBytes(bytesPerSecond)}/s\`;
        }

        // 页面加载完成后自动连接
        window.addEventListener('load', connect);
    </script>
</body>
</html>
    `;

    return new Response(html, {
      headers: {
        "Content-Type": "text/html; charset=utf-8"
      }
    });
  }
}

// 主函数
async function main() {
  const args = Deno.args;
  const port = parseInt(args[0]) || 8080;

  if (args.includes("--help") || args.includes("-h")) {
    console.log(`
系统监控服务器

用法:
  deno run -A main.ts [端口]
  deno run -A main.ts 3000

选项:
  --help, -h    显示帮助信息

默认端口: 8080

端点:
  GET /      网页监控界面
  GET /sse   SSE 流式监控
  GET /api   JSON API

示例:
  # 启动服务器
  deno run -A main.ts

  # 使用 curl 测试 SSE
  curl -N http://localhost:8080/sse

  # 使用 curl 测试 API
  curl http://localhost:8080/api
    `);
    return;
  }

  const server = new SystemMonitorServer(port);
  await server.start();
}

if (import.meta.main) {
  await main();
}