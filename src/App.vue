<script setup>
import { reactive, toRefs, onMounted, onUnmounted } from "vue";
import { fetch } from "@tauri-apps/plugin-http";
import WebSocket from "@tauri-apps/plugin-websocket";
import { getCurrentWindow } from "@tauri-apps/api/window";

// --- 1. 类型定义与常量 ---
/**
 * @typedef {'disconnected' | 'connecting' | 'connected'} ConnectionStatus
 */

/**
 * @typedef {Object} PriceState
 * @property {string} xauPrice - XAU 价格
 * @property {string} minshengPrice - 民生银行价格
 * @property {string} icbcPrice - 工商银行价格
 * @property {string} zheshangPrice - 浙商银行价格
 * @property {ConnectionStatus} wsStatus - WebSocket 连接状态
 */

/** @type {Readonly<{HTTP_FETCH_INTERVAL: number, WS_RECONNECT_INTERVAL: number, WS_URL: string}>} */
const CONSTANTS = {
  HTTP_FETCH_INTERVAL: 3000, // HTTP 请求间隔 (3秒)
  WS_RECONNECT_INTERVAL: 5 * 60 * 1000, // WebSocket 自动重连间隔 (5分钟)
  WS_URL: "wss://webhqv1.jrjr.com:39920/ws",
};

// --- 2. 响应式状态 ---
/** @type {PriceState} */
const state = reactive({
  xauPrice: "--",
  minshengPrice: "--",
  icbcPrice: "--",
  zheshangPrice: "--",
  wsStatus: "disconnected",
});

// 使用 toRefs 使模板中可以直接使用 state 的属性，且保持响应性
const { xauPrice, minshengPrice, icbcPrice, zheshangPrice, wsStatus } = toRefs(state);

// --- 3. 工具函数 ---

/**
 * 节流函数
 * @template T
 * @param {(...args: T[]) => void} func - 需要节流的函数
 * @param {number} delay - 节流延迟，单位毫秒
 * @returns {(...args: T[]) => void} 节流后的函数
 */
function throttle(func, delay) {
  let timer = null;
  return function (...args) {
    if (!timer) {
      func.apply(this, args);
      timer = setTimeout(() => {
        timer = null;
      }, delay);
    }
  };
}

// --- 4. HTTP 请求逻辑 ---

/**
 * 获取民生银行黄金价格
 */
const fetchMinshengPrice = async () => {
  try {
    const response = await fetch("https://api.jdjygold.com/gw/generic/hj/h5/m/latestPrice", {
      method: "GET",
      timeout: 30,
    });
    const data = await response.json();
    state.minshengPrice = data.resultData?.datas?.price || "获取失败";
  } catch (error) {
    console.error("获取民生银行价格失败:", error);
    state.minshengPrice = "获取失败";
  }
};

/**
 * 获取浙商银行黄金价格
 */
const fetchZheshangPrice = async () => {
  try {
    const response = await fetch("https://api.jdjygold.com/gw2/generic/jrm/h5/m/stdLatestPrice?productSku=1961543816", {
      method: "POST",
      data: { reqData: { productSku: "1961543816" } },
      timeout: 30,
    });
    const data = await response.json();
    state.zheshangPrice = data.resultData?.datas?.price || "获取失败";
  } catch (error) {
    console.error("获取浙商银行价格失败:", error);
    state.zheshangPrice = "获取失败";
  }
};

/**
 * 获取工商银行黄金价格
 */
const fetchIcbcPrice = async () => {
  try {
    const response = await fetch("https://api.jdjygold.com/gw2/generic/jrm/h5/m/icbcLatestPrice?productSku=2005453243", {
      method: "POST",
      data: { reqData: { productSku: "2005453243" } },
      timeout: 30,
    });
    const data = await response.json();
    state.icbcPrice = data.resultData?.datas?.price || "获取失败";
  } catch (error) {
    console.error("获取工商银行价格失败:", error);
    state.icbcPrice = "获取失败";
  }
};

/**
 * 并行获取所有 HTTP 价格数据
 */
const fetchAllHttpPrices = async () => {
  // 使用 Promise.all 并行执行，提高效率
  await Promise.all([fetchMinshengPrice(), fetchZheshangPrice(), fetchIcbcPrice()]);
};

// --- 5. WebSocket 逻辑 ---

let ws = null;
let reconnectTimer = null;
let httpIntervalId = null;

/**
 * 初始化或重连 WebSocket 连接
 */
const initWebsocket = async () => {
  // 如果正在连接中，则忽略重复调用
  if (state.wsStatus === "connecting") return;

  state.wsStatus = "connecting";

  // 清除旧的重连定时器
  if (reconnectTimer) {
    clearInterval(reconnectTimer);
    reconnectTimer = null;
  }

  try {
    // 关闭旧连接
    if (ws) {
      ws.removeAllListeners();
      await ws.close().catch(e => console.warn("关闭旧 WebSocket 连接时出错:", e));
      ws = null;
    }

    // 建立新连接
    ws = await WebSocket.connect(CONSTANTS.WS_URL);
    console.log("WebSocket 连接成功");
    state.wsStatus = "connected";
    state.xauPrice = "--"; // 重置价格

    // 使用 throttle 确保 UI 更新频率不会过高
    const handleMessage = throttle((event) => {
      try {
        if (event.data) {
          const data = JSON.parse(event.data);
          // 确保数据是数组且包含我们需要的 XAU 数据
          if (Array.isArray(data) && data.length > 0 && data[0].c === "XAU") {
            state.xauPrice = data[0].a;
          }
        }
      } catch (error) {
        console.error("解析 WebSocket 消息失败:", error, "原始消息:", event.data);
      }
    }, 100); // 每100ms最多处理一次

    ws.addListener("message", handleMessage);

    ws.addListener("close", (event) => {
      console.log(`WebSocket 连接关闭 (代码: ${event.code}, 原因: ${event.reason})，将尝试重连...`);
      state.wsStatus = "disconnected";
      // 使用 setTimeout 进行重连，避免立即重连可能导致的问题
      reconnectTimer = setTimeout(initWebsocket, 3000);
    });

    ws.addListener("error", (error) => {
      console.error("WebSocket 发生错误:", error);
      state.wsStatus = "disconnected";
    });

    // 设置定期重连机制（心跳重连）
    reconnectTimer = setInterval(() => {
      console.log("触发定期 WebSocket 重连...");
      initWebsocket();
    }, CONSTANTS.WS_RECONNECT_INTERVAL);

  } catch (error) {
    console.error("WebSocket 连接失败:", error);
    state.wsStatus = "disconnected";
    // 连接失败后，延迟重连
    reconnectTimer = setTimeout(initWebsocket, 5000);
  }
};

// --- 6. 生命周期管理 ---

onMounted(() => {
  // 应用启动时执行一次
  fetchAllHttpPrices();
  // 设置 HTTP 定期轮询
  httpIntervalId = setInterval(fetchAllHttpPrices, CONSTANTS.HTTP_FETCH_INTERVAL);
  // 初始化 WebSocket 连接
  initWebsocket();
});

onUnmounted(() => {
  // 组件销毁时清理资源，防止内存泄漏
  if (httpIntervalId) clearInterval(httpIntervalId);
  if (reconnectTimer) clearTimeout(reconnectTimer);
  if (ws) {
    ws.removeAllListeners();
    ws.close().catch(e => console.warn("组件销毁时关闭 WebSocket 连接出错:", e));
  }
});

// --- 7. UI 交互函数 ---

/**
 * 关闭应用窗口
 */
const handleCloseWindow = async () => {
  await getCurrentWindow().close();
};
</script>

<template>
  <div class="app-container">
    <!-- 标题栏区域，整个区域可拖拽 -->
    <div class="title-bar" data-tauri-drag-region>
      <span class="window-title">黄金价格实时监控</span>
      <button class="close-btn" @click="handleCloseWindow" title="关闭">✕</button>
    </div>

    <!-- 内容区域 -->
    <div class="content">
      <div class="price-item">
        <span class="label">XAU (WebSocket):</span>
        <span class="value" :class="{ 'fetching': wsStatus === 'connecting' }">
          {{ xauPrice }}
          <span class="ws-status-indicator" :title="wsStatus === 'connected' ? '已连接' : wsStatus === 'connecting' ? '连接中...' : '已断开'">
            {{ wsStatus === 'connected' ? '●' : wsStatus === 'connecting' ? '○' : '✕' }}
          </span>
        </span>
      </div>
      <div class="price-item">
        <span class="label">民生银行:</span>
        <span class="value">{{ minshengPrice }}</span>
      </div>
      <div class="price-item">
        <span class="label">工商银行:</span>
        <span class="value">{{ icbcPrice }}</span>
      </div>
      <div class="price-item">
        <span class="label">浙商银行:</span>
        <span class="value">{{ zheshangPrice }}</span>
      </div>
    </div>
  </div>
</template>

<style>
/* 基础与布局 */
* {
  padding: 0;
  margin: 0;
  box-sizing: border-box;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
}

body {
  background-color: transparent; /* 允许窗口透明 */
}

.app-container {
  width: 220px;
  background-color: #f0f2f5;
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  overflow: hidden; /* 确保圆角生效 */
  color: #333;
}

/* 标题栏 */
.title-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background-color: #2c3e50;
  color: white;
  font-size: 14px;
  cursor: default;
}

.window-title {
  font-weight: 500;
}

.close-btn {
  background: none;
  border: none;
  color: white;
  font-size: 18px;
  cursor: pointer;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  transition: background-color 0.2s;
}

.close-btn:hover {
  background-color: rgba(255, 255, 255, 0.2);
}

/* 内容区域 */
.content {
  padding: 15px;
}

.price-item {
  display: flex;
  justify-content: space-between;
  padding: 6px 0;
  border-bottom: 1px solid #e0e0e0;
}

.price-item:last-child {
  border-bottom: none;
}

.label {
  font-size: 14px;
  color: #666;
}

.value {
  font-size: 16px;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 6px;
}

/* WebSocket 状态指示器 */
.ws-status-indicator {
  font-size: 12px;
}
.ws-status-indicator[data-status="connected"] {
  color: #2ecc71;
}
.ws-status-indicator[data-status="connecting"] {
  color: #f39c12;
  animation: pulse 1.5s infinite;
}
.ws-status-indicator[data-status="disconnected"] {
  color: #e74c3c;
}

@keyframes pulse {
  0% { opacity: 0.4; }
  50% { opacity: 1; }
  100% { opacity: 0.4; }
}

/* 亮色/暗色主题适配 */
@media (prefers-color-scheme: dark) {
  .app-container {
    background-color: #1e1e2f;
    color: #eee;
  }
  .title-bar {
    background-color: #1a1a2e;
  }
  .price-item {
    border-bottom-color: #333;
  }
  .label {
    color: #aaa;
  }
}
</style>