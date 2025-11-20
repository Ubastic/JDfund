<script setup>
import { reactive } from "vue";
import { fetch } from "@tauri-apps/plugin-http";
import WebSocket from "@tauri-apps/plugin-websocket";
import { getCurrentWindow } from "@tauri-apps/api/window";
var ws;
const state = reactive({
  price: "",
  priceZS: "",
  priceGH:"",
  color: "",
  color2: "",
  priceDP: "",
  priceld:"",
  
});

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

 
let reconnectTimer = null;

 

const initWebsocket = async () => {
    // 清除可能存在的旧定时器
    if (reconnectTimer) {
        clearInterval(reconnectTimer);
    }

    try {
        // 关闭可能存在的旧连接
        if (ws) {
            ws.removeAllListeners();
            await ws.close(); // 确保关闭旧连接
            ws = null;
        }

        // 建立新连接
        ws = await WebSocket.connect('wss://webhqv1.jrjr.com:39920/ws');
        console.log('WebSocket连接成功');

        const handleMessage = throttle((msg) => {
            if (msg.data) {
                let data = JSON.parse(msg.data);
                if (data && data.length) {
                    if (data[0].c === 'XAU') {
                        state.priceld = data[0].a;
                        console.log('Received Message:', data[0].a);
                    }
                }
            }
        }, 300); // 每300毫秒处理一次消息

        ws.addListener(handleMessage);

        // 监听连接关闭事件，意外断开时也尝试重连
        ws.addListener('close', () => {
            console.log('WebSocket连接已关闭，将尝试重连');
            // 立即尝试重连
            initWebsocket();
        });

        // 设置5分钟后自动重连（5分钟 = 300000毫秒）
        reconnectTimer = setInterval(() => {
            console.log('5分钟自动重连触发');
            initWebsocket();
        }, 300000);

    } catch (error) {
        console.error('WebSocket连接失败:', error);
        // 连接失败时，10秒后重试
        setTimeout(initWebsocket, 10000);
    }
};
 
const getJDPrice = async () => {
  const data = await fetch(
    "https://api.jdjygold.com/gw/generic/hj/h5/m/latestPrice",
    {
      method: "GET",
      timeout: 30,
    }
  ).then((response) => response.json());
  state.price = data.resultData.datas.price;
};
const getJDPriceZS = async () => {
  const data = await fetch(
    "https://api.jdjygold.com/gw2/generic/jrm/h5/m/stdLatestPrice?productSku=1961543816",
    {
      method: "post",
      data: {
        reqData: { productSku: "1961543816" },
      },
      timeout: 30,
    }
  ).then((response) => response.json());
  state.priceZS = data.resultData.datas.price;
};

const getJDPriceGH = async () => {
  const data = await fetch(
    "https://api.jdjygold.com/gw2/generic/jrm/h5/m/icbcLatestPrice?productSku=2005453243",
    {
      method: "post",
      data: {
        reqData: { productSku: "2005453243" },
      },
      timeout: 30,
    }
  ).then((response) => response.json());
  state.priceGH = data.resultData.datas.price;
};


const getPrice = async () => {
  getJDPrice();
  getJDPriceZS();
  getJDPriceGH()
};
getPrice();
setInterval(() => {
  getPrice();
}, 3000);
initWebsocket()
const close = async () => {
  await getCurrentWindow().close();
};
</script>

<template>
    <div data-tauri-drag-region style="margin-top: 3px; cursor: default" class="red">
    XAU：{{ state.priceld }}
  </div>
  <div data-tauri-drag-region style="margin-top: 3px; cursor: default" class="red">
    民生：{{ state.price }}
  </div>
   <div data-tauri-drag-region style="margin-top: 3px; cursor: default" class="red">
    工行：{{ state.priceGH }}
  </div>
  <div data-tauri-drag-region style="margin-top: 0px; cursor: default" class="red">
    浙商：{{ state.priceZS }}
  </div>
  
  <div class="close" @click="close">关闭</div>
</template>

<style>
* {
  padding: 0;
  margin: 0;
}

@media (prefers-color-scheme: dark) {
  html {
    background-color: #2f2f2f;
    padding-left: 10px;
  }
}

/* 当系统处于亮色模式（偏好亮色主题）时应用以下样式 */
@media (prefers-color-scheme: light) {
  html {
    background-color: white;
    padding-left: 10px;
  }
}

html :hover .close {
  display: block;
}

.red {
  color: red;
}

.green {
  color: green;
}

.close {
  position: absolute;
  right: 10px;
  top: 10px;
  cursor: pointer;
  color: white;
  display: none;
}
</style>
