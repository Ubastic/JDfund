<template>
  <div class="price-float" :style="{ backgroundColor: settings.bgColor }" data-tauri-drag-region>
    <div class="prices">
      <span v-if="settings.showXAU" class="price-tag">{{ xauPrice }}</span>
      <span v-if="settings.showMS" class="price-tag">{{ minshengPrice }}</span>
      <span v-if="settings.showGH" class="price-tag">{{ icbcPrice }}</span>
      <span v-if="settings.showZS" class="price-tag">{{ zheshangPrice }}</span>
    </div>
  </div>
</template>

<script setup>
import { reactive, toRefs, onMounted, onUnmounted, ref } from "vue";
import { fetch } from "@tauri-apps/plugin-http";
import WebSocket from '@tauri-apps/plugin-websocket';
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

// 设置 - 从后端加载
const settings = ref({
  showXAU: true,
  showMS: true,
  showGH: true,
  showZS: true,
  bgColor: '#2c3e50'
});

// 价格状态
const state = reactive({
  xauPrice: "--",
  minshengPrice: "--",
  icbcPrice: "--",
  zheshangPrice: "--"
});

const { xauPrice, minshengPrice, icbcPrice, zheshangPrice } = toRefs(state);

// 常量
const CONSTANTS = {
  HTTP_FETCH_INTERVAL: 3000,
  WS_URL: "wss://cfws.jdjygold.com/data"
};

// HTTP 请求
const fetchMinshengPrice = async () => {
  try {
    const response = await fetch("https://api.jdjygold.com/gw/generic/hj/h5/m/latestPrice", {
      method: "GET",
      timeout: 30
    });
    const data = await response.json();
    state.minshengPrice = data.resultData?.datas?.price || "--";
  } catch (error) {
    state.minshengPrice = "--";
  }
};

const fetchZheshangPrice = async () => {
  try {
    const response = await fetch("https://api.jdjygold.com/gw2/generic/jrm/h5/m/stdLatestPrice?productSku=1961543816", {
      method: "POST",
      data: { reqData: { productSku: "1961543816" } },
      timeout: 30
    });
    const data = await response.json();
    state.zheshangPrice = data.resultData?.datas?.price || "--";
  } catch (error) {
    state.zheshangPrice = "--";
  }
};

const fetchIcbcPrice = async () => {
  try {
    const response = await fetch("https://api.jdjygold.com/gw2/generic/jrm/h5/m/icbcLatestPrice?productSku=2005453243", {
      method: "POST",
      data: { reqData: { productSku: "2005453243" } },
      timeout: 30
    });
    const data = await response.json();
    state.icbcPrice = data.resultData?.datas?.price || "--";
  } catch (error) {
    state.icbcPrice = "--";
  }
};

const fetchAllHttpPrices = async () => {
  await Promise.all([fetchMinshengPrice(), fetchZheshangPrice(), fetchIcbcPrice()]);
};

// WebSocket
let ws = null;
let httpIntervalId = null;

const initWebsocket = async () => {
  try {
    ws = await WebSocket.connect(CONSTANTS.WS_URL);
    ws.send(JSON.stringify({ "action": "2", "bizType": "2", "keys": ["WG-XAUUSD"] }));
    ws.addListener((e) => {
      try {
        let data = JSON.parse(e.data);
        if (data.data?.lastPrice) {
          state.xauPrice = data.data.lastPrice;
        }
      } catch (err) {}
    });
  } catch (err) {}
};

// 监听设置更新
let unlisten = null;

onMounted(async () => {
  const win = getCurrentWindow();
  await win.setAlwaysOnTop(true);
  
  // 从后端加载设置
  try {
    const backendSettings = await invoke('get_settings');
    settings.value = {
      showXAU: backendSettings.show_xau,
      showMS: backendSettings.show_ms,
      showGH: backendSettings.show_gh,
      showZS: backendSettings.show_zs,
      bgColor: backendSettings.bg_color
    };
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
  
  // 监听设置更新事件
  unlisten = await listen('settings-updated', (event) => {
    const s = event.payload;
    settings.value = {
      showXAU: s.show_xau,
      showMS: s.show_ms,
      showGH: s.show_gh,
      showZS: s.show_zs,
      bgColor: s.bg_color
    };
  });
  
  // 初始获取价格
  fetchAllHttpPrices();
  httpIntervalId = setInterval(fetchAllHttpPrices, CONSTANTS.HTTP_FETCH_INTERVAL);
  initWebsocket();
});

onUnmounted(() => {
  if (httpIntervalId) clearInterval(httpIntervalId);
  if (unlisten) unlisten();
});
</script>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  background: transparent;
  margin: 0;
  overflow: hidden;
}

.price-float {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 6px 12px;
  border-radius: 4px;
  cursor: move;
  user-select: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  min-width: fit-content;
}

.prices {
  display: flex;
  gap: 8px;
  align-items: center;
}

.price-tag {
  color: #ffd700;
  font-size: 14px;
  font-weight: 600;
  font-family: 'Consolas', 'Monaco', monospace;
  white-space: nowrap;
}
</style>