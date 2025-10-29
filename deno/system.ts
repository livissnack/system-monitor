// system_info.ts
export interface SystemInfo {
  system: {
    platform: string;
    arch: string;
    hostname: string;
    pid: number;
    ppid: number;
    uid: number;
    gid: number;
    home: string;
    cwd: string;
    uptime: number;
    bootTime: number;
  };
  memory: {
    total: number;
    available: number;
    free: number;
    used: number;
    usage: number;
  };
  cpu: {
    cores: number;
    arch: string;
    usage: number;
    loadAverage: number[];
  };
  disk: {
    total: number;
    free: number;
    used: number;
    usage: number;
  };
  network: {
    interfaces: NetworkInterface[];
    connections?: ProcessConnection[];
    stats: NetworkStats;
  };
  runtime: {
    denoVersion: string;
    v8Version: string;
    typescriptVersion: string;
    execPath: string;
    startTime: number;
  };
  process: {
    argv: string[];
    execPath: string;
    memory: Deno.MemoryUsage;
    pid: number;
    uptime: number;
  };
  timestamp: number;
}

export interface NetworkInterface {
  name: string;
  address: string;
  netmask?: string;
  family: "IPv4" | "IPv6";
  mac?: string;
  scopeid?: number;
  cidr: string;
}

export interface ProcessConnection {
  localAddr: Deno.NetAddr;
  remoteAddr: Deno.NetAddr;
  state: string;
  pid: number;
}

export interface NetworkStats {
  bytesReceived: number;
  bytesSent: number;
  packetsReceived: number;
  packetsSent: number;
  uploadSpeed: number;
  downloadSpeed: number;
}

export interface SystemMonitorOptions {
  interval?: number;
  enableCpuUsage?: boolean;
  enableNetworkStats?: boolean;
}

// 全局变量存储上次的统计信息
let lastCpuUsage: { user: number; system: number } | null = null;
let lastNetworkStats: NetworkStats | null = null;
let lastNetworkTime: number | null = null;
let monitorStartTime: number = Date.now();

// 使用新的 Deno.Command 执行系统命令
async function executeCommand(cmd: string[], input?: string): Promise<{ success: boolean; output: string; error: string }> {
  try {
    const command = new Deno.Command(cmd[0], {
      args: cmd.slice(1),
      stdin: input ? "piped" : "null",
      stdout: "piped",
      stderr: "piped",
    });

    const process = command.spawn();
    
    if (input) {
      const writer = process.stdin.getWriter();
      await writer.write(new TextEncoder().encode(input));
      await writer.close();
    }

    const { success, stdout, stderr } = await process.output();
    
    return {
      success,
      output: new TextDecoder().decode(stdout),
      error: new TextDecoder().decode(stderr),
    };
  } catch (error) {
    return {
      success: false,
      output: "",
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

// 获取 CPU 使用率（跨平台实现）
async function getCpuUsage(): Promise<number> {
  try {
    if (Deno.build.os === "linux") {
      const stat = await Deno.readTextFile("/proc/stat");
      const lines = stat.split("\n");
      const cpuLine = lines[0];
      
      if (cpuLine.startsWith("cpu ")) {
        const parts = cpuLine.split(/\s+/).slice(1);
        const user = parseInt(parts[0]);
        const nice = parseInt(parts[1]);
        const system = parseInt(parts[2]);
        const idle = parseInt(parts[3]);
        const iowait = parseInt(parts[4]) || 0;
        
        const total = user + nice + system + idle + iowait;
        const used = user + nice + system;
        
        if (lastCpuUsage) {
          const totalDiff = total - (lastCpuUsage.user + lastCpuUsage.system);
          const usedDiff = used - lastCpuUsage.user;
          const usage = totalDiff > 0 ? (usedDiff / totalDiff) * 100 : 0;
          
          lastCpuUsage = { user: used, system: total - used };
          return Math.min(Math.max(usage, 0), 100);
        } else {
          lastCpuUsage = { user: used, system: total - used };
          return 0;
        }
      }
    } else if (Deno.build.os === "darwin") {
      const result = await executeCommand(["top", "-l", "1", "-n", "0"]);
      if (result.success) {
        const lines = result.output.split('\n');
        for (const line of lines) {
          if (line.includes("CPU usage")) {
            const match = line.match(/(\d+\.\d+)% user/);
            if (match) {
              return parseFloat(match[1]);
            }
          }
        }
      }
    } else if (Deno.build.os === "windows") {
      const result = await executeCommand([
        "wmic", "cpu", "get", "LoadPercentage", "/value"
      ]);
      if (result.success) {
        const match = result.output.match(/LoadPercentage=(\d+)/);
        if (match) {
          return parseInt(match[1]);
        }
      }
    }
  } catch (error) {
    console.warn("无法获取 CPU 使用率:", error instanceof Error ? error.message : String(error));
  }
  
  return 0;
}

// 获取网络统计信息
async function getNetworkStats(): Promise<NetworkStats> {
  const stats: NetworkStats = {
    bytesReceived: 0,
    bytesSent: 0,
    packetsReceived: 0,
    packetsSent: 0,
    uploadSpeed: 0,
    downloadSpeed: 0
  };

  try {
    if (Deno.build.os === "linux") {
      const netDev = await Deno.readTextFile("/proc/net/dev");
      const lines = netDev.split('\n');
      
      for (const line of lines.slice(2)) { // 跳过前两行标题
        const parts = line.trim().split(/\s+/);
        if (parts.length >= 10) {
          const interfaceName = parts[0].replace(':', '');
          // 排除回环接口
          if (interfaceName !== 'lo') {
            stats.bytesReceived += parseInt(parts[1]);
            stats.packetsReceived += parseInt(parts[2]);
            stats.bytesSent += parseInt(parts[9]);
            stats.packetsSent += parseInt(parts[10]);
          }
        }
      }
    } else if (Deno.build.os === "darwin") {
      const result = await executeCommand(["netstat", "-ib"]);
      if (result.success) {
        const lines = result.output.split('\n');
        for (const line of lines) {
          const parts = line.trim().split(/\s+/);
          if (parts.length >= 7 && parts[0] !== 'Name' && !parts[0].startsWith('lo')) {
            stats.bytesReceived += parseInt(parts[6]) || 0;
            stats.bytesSent += parseInt(parts[9]) || 0;
          }
        }
      }
    } else if (Deno.build.os === "windows") {
      const result = await executeCommand([
        "netstat", "-e"
      ]);
      if (result.success) {
        const lines = result.output.split('\n');
        for (const line of lines) {
          if (line.includes("Bytes")) {
            const parts = line.trim().split(/\s+/);
            if (parts.length >= 3) {
              stats.bytesReceived = parseInt(parts[1]) || 0;
              stats.bytesSent = parseInt(parts[2]) || 0;
            }
          }
        }
      }
    }
    
    // 计算网络速度
    const now = Date.now();
    if (lastNetworkStats && lastNetworkTime) {
      const timeDiff = (now - lastNetworkTime) / 1000; // 秒
      if (timeDiff > 0) {
        stats.uploadSpeed = (stats.bytesSent - lastNetworkStats.bytesSent) / timeDiff;
        stats.downloadSpeed = (stats.bytesReceived - lastNetworkStats.bytesReceived) / timeDiff;
      }
    }
    
    lastNetworkStats = { ...stats };
    lastNetworkTime = now;
    
  } catch (error) {
    console.warn("无法获取网络统计信息:", error instanceof Error ? error.message : String(error));
  }
  
  return stats;
}

// 获取系统启动时间
async function getBootTime(): Promise<number> {
  try {
    if (Deno.build.os === "linux") {
      const uptime = await Deno.readTextFile("/proc/uptime");
      const systemUptime = parseFloat(uptime.split(' ')[0]);
      return Date.now() - (systemUptime * 1000);
    } else if (Deno.build.os === "darwin") {
      const result = await executeCommand(["sysctl", "-n", "kern.boottime"]);
      if (result.success) {
        const match = result.output.match(/sec = (\d+)/);
        if (match) {
          return parseInt(match[1]) * 1000;
        }
      }
    }
  } catch (error) {
    console.warn("无法获取启动时间:", error instanceof Error ? error.message : String(error));
  }
  
  return Date.now() - (monitorStartTime - performance.timeOrigin);
}

// 获取负载平均值
async function getLoadAverage(): Promise<number[]> {
  try {
    if (Deno.build.os === "linux" || Deno.build.os === "darwin") {
      const result = await executeCommand(["uptime"]);
      if (result.success) {
        const match = result.output.match(/load averages?: ([\d.]+),? ([\d.]+),? ([\d.]+)/);
        if (match) {
          return [parseFloat(match[1]), parseFloat(match[2]), parseFloat(match[3])];
        }
      }
    }
  } catch (error) {
    console.warn("无法获取负载平均值:", error instanceof Error ? error.message : String(error));
  }
  
  return [0, 0, 0];
}

// 获取完整的系统信息
export async function getSystemInfo(options: SystemMonitorOptions = {}): Promise<SystemInfo> {
  const [cpuUsage, networkStats, bootTime, loadAverage] = await Promise.all([
    options.enableCpuUsage ? getCpuUsage() : Promise.resolve(0),
    options.enableNetworkStats ? getNetworkStats() : Promise.resolve({
      bytesReceived: 0, bytesSent: 0, packetsReceived: 0, packetsSent: 0,
      uploadSpeed: 0, downloadSpeed: 0
    }),
    getBootTime(),
    getLoadAverage()
  ]);

  return {
    system: await getSystemInfoInternal(bootTime),
    memory: await getMemoryInfo(),
    cpu: getCpuInfo(cpuUsage, loadAverage),
    disk: await getDiskInfo(),
    network: await getNetworkInfo(networkStats),
    runtime: getRuntimeInfo(),
    process: getProcessInfo(),
    timestamp: Date.now(),
  };
}

// 系统基本信息
async function getSystemInfoInternal(bootTime: number) {
  return {
    platform: Deno.build.os,
    arch: Deno.build.arch,
    hostname: Deno.hostname(),
    pid: Deno.pid,
    ppid: Deno.ppid,
    uid: Deno.uid() ?? -1,
    gid: Deno.gid() ?? -1,
    home: Deno.env.get("HOME") || Deno.env.get("USERPROFILE") || "",
    cwd: Deno.cwd(),
    uptime: Date.now() - bootTime,
    bootTime,
  };
}

// 内存信息
async function getMemoryInfo() {
  let totalMemory = 0;
  let freeMemory = 0;

  try {
    if (Deno.build.os === "linux") {
      const memInfo = await Deno.readTextFile("/proc/meminfo");
      const lines = memInfo.split("\n");
      
      for (const line of lines) {
        if (line.startsWith("MemTotal:")) {
          totalMemory = parseInt(line.split(/\s+/)[1]) * 1024;
        } else if (line.startsWith("MemAvailable:")) {
          freeMemory = parseInt(line.split(/\s+/)[1]) * 1024;
        }
      }
    } else if (Deno.build.os === "darwin") {
      const result = await executeCommand(["sysctl", "-n", "hw.memsize"]);
      if (result.success) {
        totalMemory = parseInt(result.output.trim());
      }
      
      const vmResult = await executeCommand(["vm_stat"]);
      if (vmResult.success) {
        const lines = vmResult.output.split("\n");
        let freePages = 0;
        
        for (const line of lines) {
          if (line.startsWith("Pages free:")) {
            const value = line.split(":")[1].trim().replace(".", "");
            freePages = parseInt(value) || 0;
            break;
          }
        }
        
        freeMemory = freePages * 4096;
      }
    } else if (Deno.build.os === "windows") {
      const result = await executeCommand([
        "wmic", "ComputerSystem", "get", "TotalPhysicalMemory", "/value"
      ]);
      
      if (result.success) {
        const match = result.output.match(/TotalPhysicalMemory=(\d+)/);
        if (match) {
          totalMemory = parseInt(match[1]);
        }
      }
      
      const freeResult = await executeCommand([
        "wmic", "OS", "get", "FreePhysicalMemory", "/value"
      ]);
      
      if (freeResult.success) {
        const freeMatch = freeResult.output.match(/FreePhysicalMemory=(\d+)/);
        if (freeMatch) {
          freeMemory = parseInt(freeMatch[1]) * 1024;
        }
      }
    }
  } catch (error) {
    console.warn("无法获取内存信息:", error instanceof Error ? error.message : String(error));
  }

  const usedMemory = totalMemory - freeMemory;
  const usage = totalMemory > 0 ? (usedMemory / totalMemory) * 100 : 0;

  return {
    total: Math.round(totalMemory / 1024 / 1024),
    available: Math.round(freeMemory / 1024 / 1024),
    free: Math.round(freeMemory / 1024 / 1024),
    used: Math.round(usedMemory / 1024 / 1024),
    usage: Math.round(usage * 100) / 100,
  };
}

// CPU 信息
function getCpuInfo(usage: number, loadAverage: number[]) {
  return {
    cores: navigator.hardwareConcurrency || 1,
    arch: Deno.build.arch,
    usage,
    loadAverage,
  };
}

// 磁盘信息
async function getDiskInfo() {
  try {
    const cwd = Deno.cwd();
    let totalSpace = 0;
    let freeSpace = 0;

    if (Deno.build.os === "linux" || Deno.build.os === "darwin") {
      const result = await executeCommand(["df", "-k", cwd]);
      if (result.success) {
        const lines = result.output.split("\n");
        if (lines.length > 1) {
          const parts = lines[1].split(/\s+/).filter(part => part.length > 0);
          if (parts.length >= 4) {
            totalSpace = parseInt(parts[1]) * 1024;
            freeSpace = parseInt(parts[3]) * 1024;
          }
        }
      }
    } else if (Deno.build.os === "windows") {
      const drive = cwd.substring(0, 2);
      const result = await executeCommand([
        "wmic", "logicaldisk", "where", `deviceid="${drive}"`, "get", "size,freespace", "/value"
      ]);
      
      if (result.success) {
        const sizeMatch = result.output.match(/Size=(\d+)/);
        const freeMatch = result.output.match(/FreeSpace=(\d+)/);
        
        if (sizeMatch) totalSpace = parseInt(sizeMatch[1]);
        if (freeMatch) freeSpace = parseInt(freeMatch[1]);
      }
    }
    
    const usedSpace = totalSpace - freeSpace;
    const usage = totalSpace > 0 ? (usedSpace / totalSpace) * 100 : 0;

    return {
      total: Math.round(totalSpace / 1024 / 1024 / 1024),
      free: Math.round(freeSpace / 1024 / 1024 / 1024),
      used: Math.round(usedSpace / 1024 / 1024 / 1024),
      usage: Math.round(usage * 100) / 100,
    };
  } catch (error) {
    console.warn("无法获取磁盘信息:", error instanceof Error ? error.message : String(error));
    return {
      total: 0,
      free: 0,
      used: 0,
      usage: 0,
    };
  }
}

// 网络信息
async function getNetworkInfo(networkStats: NetworkStats) {
  const interfaces: NetworkInterface[] = [];
  
  try {
    const connections = [];
    
    if (Deno.build.os === "linux") {
      const result = await executeCommand(["ss", "-tunlp"]);
      if (result.success) {
        connections.push(...parseNetworkConnections(result.output));
      } else {
        const netstatResult = await executeCommand(["netstat", "-tunlp"]);
        if (netstatResult.success) {
          connections.push(...parseNetworkConnections(netstatResult.output));
        }
      }
    }
    
    return {
      interfaces,
      connections,
      stats: networkStats,
    };
  } catch (error) {
    console.warn("无法获取网络信息:", error instanceof Error ? error.message : String(error));
    return {
      interfaces: [],
      connections: [],
      stats: networkStats,
    };
  }
}

// 解析网络连接信息
function parseNetworkConnections(output: string): ProcessConnection[] {
  const connections: ProcessConnection[] = [];
  const lines = output.split("\n");
  
  for (const line of lines.slice(1)) {
    const parts = line.trim().split(/\s+/);
    if (parts.length >= 6) {
      const [localIp, localPort] = parts[3].split(":");
      const [remoteIp, remotePort] = parts[4].split(":");
      
      if (localIp && localPort && remoteIp && remotePort) {
        connections.push({
          localAddr: {
            hostname: localIp,
            port: parseInt(localPort),
            transport: "tcp" as const,
          },
          remoteAddr: {
            hostname: remoteIp,
            port: parseInt(remotePort),
            transport: "tcp" as const,
          },
          state: parts[1],
          pid: parseInt(parts[6].split("/")[0]) || 0,
        });
      }
    }
  }
  
  return connections;
}

// 运行时信息
function getRuntimeInfo() {
  return {
    denoVersion: Deno.version.deno,
    v8Version: Deno.version.v8,
    typescriptVersion: Deno.version.typescript,
    execPath: Deno.execPath(),
    startTime: performance.timeOrigin,
  };
}

// 进程信息
function getProcessInfo() {
  return {
    argv: Deno.args,
    execPath: Deno.execPath(),
    memory: Deno.memoryUsage(),
    pid: Deno.pid,
    uptime: performance.now(),
  };
}

// 格式化字节大小
export function formatBytes(bytes: number): string {
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

// 格式化时间
export function formatUptime(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000);
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;

  const parts = [];
  if (days > 0) parts.push(`${days}天`);
  if (hours > 0) parts.push(`${hours}小时`);
  if (minutes > 0) parts.push(`${minutes}分`);
  parts.push(`${secs}秒`);

  return parts.join(' ');
}

// SSE 流监控
export class SystemMonitorSSE {
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  private intervalId: number | null = null;
  private options: SystemMonitorOptions;

  constructor(options: SystemMonitorOptions = {}) {
    this.options = {
      interval: 2000,
      enableCpuUsage: true,
      enableNetworkStats: true,
      ...options
    };
  }

  // 创建 SSE 流
  createStream(): ReadableStream<Uint8Array> {
    return new ReadableStream({
      start: (controller) => {
        this.controller = controller;
        this.startMonitoring();
      },
      cancel: () => {
        this.stopMonitoring();
      }
    });
  }

  private async startMonitoring() {
    // 发送初始数据
    await this.sendData();

    // 开始定时监控
    this.intervalId = setInterval(async () => {
      await this.sendData();
    }, this.options.interval);
  }

  private stopMonitoring() {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }
    this.controller = null;
  }

  private async sendData() {
    if (!this.controller) return;

    try {
      const systemInfo = await getSystemInfo(this.options);
      const data = JSON.stringify({
        type: 'system_info',
        data: systemInfo,
        timestamp: Date.now()
      });

      const message = `data: ${data}\n\n`;
      this.controller.enqueue(new TextEncoder().encode(message));
    } catch (error) {
      const errorData = JSON.stringify({
        type: 'error',
        error: error instanceof Error ? error.message : String(error),
        timestamp: Date.now()
      });
      
      const message = `data: ${errorData}\n\n`;
      this.controller.enqueue(new TextEncoder().encode(message));
    }
  }
}