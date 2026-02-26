<template>
  <div 
    class="price-float" 
    :class="{ 'docked': isDocked, 'price-changed': priceJustChanged }" 
    :style="{ backgroundColor: isDocked ? 'transparent' : settings.bgColor, opacity: computedOpacity }" 
    data-tauri-drag-region
    @mouseenter="handleMouseEnter"
    @mouseleave="handleMouseLeave"
  >
    <div class="prices" :class="{ 'docked-content': isDocked }">
      <span v-if="settings.showXAU" class="price-tag" :class="{ 'docked-price': isDocked }">{{ xauPrice }}</span>
      <span v-if="settings.showMS" class="price-tag" :class="{ 'docked-price': isDocked }">{{ minshengPrice }}</span>
      <span v-if="settings.showGH" class="price-tag" :class="{ 'docked-price': isDocked }">{{ icbcPrice }}</span>
      <span v-if="settings.showZS" class="price-tag" :class="{ 'docked-price': isDocked }">{{ zheshangPrice }}</span>
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

// 智能淡显状态
const isHovered = ref(false);
const isDocked = ref(false);
const priceJustChanged = ref(false);
const normalOpacity = ref(0.35); // 平时透明度
const hoverOpacity = ref(1.0);  // 悬停透明度
const priceChangeTimer = ref(null);

// 计算当前透明度
const computedOpacity = ref(normalOpacity.value);

// 价格状态
const state = reactive({
  xauPrice: "--",
  minshengPrice: "--",
  icbcPrice: "--",
  zheshangPrice: "--"
});

// 记录上次价格用于检测变动
const lastPrices = ref({
  xau: null,
  ms: null,
  gh: null,
  zs: null
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
    const newPrice = data.resultData?.datas?.price;
    if (newPrice && newPrice !== lastPrices.value.ms) {
      triggerPriceChange();
      lastPrices.value.ms = newPrice;
    }
    state.minshengPrice = newPrice || "--";
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
    const newPrice = data.resultData?.datas?.price;
    if (newPrice && newPrice !== lastPrices.value.zs) {
      triggerPriceChange();
      lastPrices.value.zs = newPrice;
    }
    state.zheshangPrice = newPrice || "--";
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
    const newPrice = data.resultData?.datas?.price;
    if (newPrice && newPrice !== lastPrices.value.gh) {
      triggerPriceChange();
      lastPrices.value.gh = newPrice;
    }
    state.icbcPrice = newPrice || "--";
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
let dockCheckInterval = null;

// 触发价格变动闪烁
const triggerPriceChange = () => {
  priceJustChanged.value = true;
  computedOpacity.value = hoverOpacity.value;
  if (priceChangeTimer.value) clearTimeout(priceChangeTimer.value);
  priceChangeTimer.value = setTimeout(() => {
    priceJustChanged.value = false;
    if (!isHovered.value && !isDocked.value) {
      computedOpacity.value = normalOpacity.value;
    }
  }, 2000);
};

// 鼠标进入 - 恢复透明度
const handleMouseEnter = () => {
  isHovered.value = true;
  computedOpacity.value = hoverOpacity.value;
};

// 鼠标离开 - 恢复淡化
const handleMouseLeave = () => {
  isHovered.value = false;
  if (!priceJustChanged.value && !isDocked.value) {
    computedOpacity.value = normalOpacity.value;
  }
};

// 检查是否贴边
const checkDockedStatus = async () => {
  try {
    const win = getCurrentWindow();
    const position = await win.outerPosition();
    const size = await win.innerSize();
    const monitor = await win.primaryMonitor();
    
    if (monitor && position && size) {
      const screenWidth = monitor.size.width;
      const screenHeight = monitor.size.height;
      const dockThreshold = 20; // 贴边阈值像素
      
      // 检查是否贴近左/右边缘
      const isLeftDocked = position.x <= dockThreshold;
      const isRightDocked = (position.x + size.width) >= (screenWidth - dockThreshold);
      const isTopDocked = position.y <= dockThreshold;
      const isBottomDocked = (position.y + size.height) >= (screenHeight - dockThreshold);
      
      // 只有贴边且不在悬停状态时才收缩
      const shouldDock = (isLeftDocked || isRightDocked || isTopDocked || isBottomDocked) && !isHovered.value;
      
      if (shouldDock !== isDocked.value) {
        isDocked.value = shouldDock;
        if (shouldDock && !priceJustChanged.value) {
          computedOpacity.value = 0.2; // 贴边时更透明
        } else if (!shouldDock && !isHovered.value && !priceJustChanged.value) {
          computedOpacity.value = normalOpacity.value;
        }
      }
    }
  } catch (e) {
    // 静默处理错误
  }
};

const initWebsocket = async () => {
  try {
    ws = await WebSocket.connect(CONSTANTS.WS_URL);
    ws.send(JSON.stringify({ "action": "2", "bizType": "2", "keys": ["WG-XAUUSD"] }));
    ws.addListener((e) => {
      try {
        let data = JSON.parse(e.data);
        if (data.data?.lastPrice) {
          const newPrice = data.data.lastPrice;
          if (newPrice !== lastPrices.value.xau) {
            triggerPriceChange();
            lastPrices.value.xau = newPrice;
          }
          state.xauPrice = newPrice;
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
  
  // 启动贴边检测（每500ms检查一次位置）
  dockCheckInterval = setInterval(checkDockedStatus, 500);
  // 初始检查一次
  setTimeout(checkDockedStatus, 1000);
});

onUnmounted(() => {
  if (httpIntervalId) clearInterval(httpIntervalId);
  if (dockCheckInterval) clearInterval(dockCheckInterval);
  if (unlisten) unlisten();
  if (priceChangeTimer.value) clearTimeout(priceChangeTimer.value);
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
  transition: font-size 0.2s ease, color 0.2s ease, transform 0.2s ease;
}

/* 贴边收缩模式 */
.price-float.docked {
  padding: 2px 4px;
  box-shadow: none;
}

.price-float.docked .prices {
  gap: 4px;
}

.price-float.docked .price-tag {
  font-size: 11px;
  font-weight: 400;
}

/* 贴边内容更紧凑 */
.docked-content {
  flex-direction: column;
  gap: 2px !important;
}

.docked-price {
  font-size: 10px !important;
  opacity: 0.8;
}

/* 价格变动闪烁动画 */
.price-float.price-changed .price-tag {
  animation: priceFlash 2s ease-out;
}

@keyframes priceFlash {
  0% { 
    color: #ff6b6b;
    text-shadow: 0 0 8px rgba(255, 107, 107, 0.8);
    transform: scale(1.05);
  }
  50% { 
    color: #ffd700;
    text-shadow: 0 0 4px rgba(255, 215, 0, 0.4);
    transform: scale(1.02);
  }
  100% { 
    color: #ffd700;
    text-shadow: none;
    transform: scale(1);
  }
}
</style>