<script setup>
import { onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router"; import {
  NPageHeader,
  NButton,
  NButtonGroup,
  NSpin,
  NText,
  NEl,
  NFormItem,
  NInput,
  NH3,
  NIcon,
  NCollapseTransition,
  NNumberAnimation,
} from "naive-ui";
import ParamsForm from "@/components/ParamsForm.vue";
import { ArrowDown16Filled } from "@vicons/fluent";
import OpenSeadragon from "openseadragon";

const route = useRoute();
const router = useRouter();


const showForms = ref(false);

function open(url) {
  url = url + "?t=" + Date.now();
  window.open(url);
}

let viewer;
function initOpenseagragon(el) {
  if (viewer) return;

  viewer = OpenSeadragon({
    element: el,
    prefixUrl:
      "https://cdn.jsdelivr.net/npm/openseadragon@4.0/build/openseadragon/images/",
  });
}

function onResize() {
  if (window.innerWidth > 768) {
    showForms.value = true;
  }
}

const data = ref({});
const loading = ref(true);

let lastParams = null;
let lastRemark = null;

async function fetcher() {
  const res = await fetch(`/api/detections/${route.params.id}`);
  const j = await res.json();
  if (j.error) {
    router.push("/");
    return;
  }
  data.value = j;
  loading.value = false;
}

async function updator() {
  const params = JSON.stringify(data.value.params);
  if (lastParams !== params) {
    lastParams = params;
    await updateParams(data.value.params);
  }

  const remark = data.value.remark;
  if (lastRemark !== remark) {
    lastRemark = remark;
    await fetch("/api/detections/" + route.params.id + "/remark", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ remark }),
    });

  }

  setTimeout(updator, 1000);
}

onMounted(async () => {
  window.addEventListener("resize", onResize);
  onResize();

  await fetcher();
  lastParams = JSON.stringify(data.value.params);
  lastRemark = data.value.remark;

  await pooler();

  viewer.open({
    type: "image",
    url: "/api/detections/" + route.params.id + "/windows?t=" + Date.now(),
  });

  updator();

  return () => {
    window.removeEventListener("resize", onResize);
  };
});

let anim = ref(null);
const lastNumber = ref(0);
const imageSpin = ref(false);

let updating = false;

async function pooler() {
  while (data.value.status !== "done") {
    imageSpin.value = true;
    await fetcher();
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  imageSpin.value = false;
}

async function updateParams(params) {
  if (updating) return;
  updating = true;

  console.log("update params", params);

  const res = await fetch("/api/detections/" + route.params.id + "/params", {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(params),
  });
  const j = await res.json();

  data.value = j;

  // Long polling
  //
  await pooler();

  console.log("ok");
  // Reload viewer
  viewer.open({
    type: "image",
    url: "/api/detections/" + route.params.id + "/windows?t=" + Date.now(),
  });
  updating = false;
  anim.value.play();
}
</script>

<template>
  <n-spin :show="loading">
    <template #description>
      <n-text depth="3">如果长时间未加载，请刷新页面</n-text>
    </template>
    <main v-if="!loading">
      <n-el tag="div" class="params" style="
          background-color: var(--card-color);
          transition: background-color 0.3s var(--cubic-bezier-ease-in-out);
        ">
        <n-page-header @back="router.push('/')" title="Swift Lite" subtitle="编辑器">
          <template #extra>
            <!-- <n-button>刷新</n-button> -->
            <n-h3 style="margin: 0">
              <n-number-animation ref="anim" :from="lastNumber" :to="data.num"
                @finish="lastNumber = data.num"></n-number-animation>
              只
            </n-h3>
          </template>
        </n-page-header>

        <n-collapse-transition :show="showForms">
          <div class="forms">
            <ParamsForm :data="data.params"></ParamsForm>

            <n-form-item label="备注">
              <n-input v-model:value="data.remark" placeholder="无"></n-input>
            </n-form-item>

            <n-button-group>
              <n-button @click="() => open(`/api/detections/${route.params.id}/origin`)">下载原图</n-button>
              <n-button @click="() => open(`/api/detections/${route.params.id}/windows`)">下载标注</n-button>
              <n-button @click="() => open(`/api/detections/${route.params.id}/boxes`)">下载标注 (无窗口)</n-button>
            </n-button-group>
          </div>
        </n-collapse-transition>

        <div class="btn">
          <n-button circle secondary @click="showForms = !showForms">
            <template #icon>
              <n-icon>
                <ArrowDown16Filled :style="{
    transform: showForms ? 'rotate(180deg)' : 'rotate(0)',
    transition: 'transform 0.3s',
  }" />
              </n-icon>
            </template>
          </n-button>
        </div>
      </n-el>

      <div class="image">
        <n-spin style="width: 100%; height: 100%;" :show="imageSpin">
          <div class="layers" :ref="initOpenseagragon"></div>
          <template #description>
            <n-text depth="3" v-if="data.status === 'queue'">排队中...
              <span v-if="data.queue && data.queue > 0">第 {{ data.queue }} 位</span>
            </n-text>
            <n-text depth="3" v-else-if="data.status === 'processing'">处理中...</n-text>
          </template>
        </n-spin>
      </div>
    </main>
    <main v-else></main>
  </n-spin>
</template>

<style scoped>
main {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.params {
  /* flex: 1; */
  width: 30rem;
  padding: 1rem;
  position: relative;
  z-index: 5;
  background: var(--);
}

.btn {
  display: none;
  position: absolute;
  bottom: -2.7rem;
  left: 50%;
  transform: translateX(-50%);
}

.forms {
  margin-top: 2rem;
}

.image {
  flex-grow: 1;
  width: 100%;
  overflow: hidden;
  min-height: 10rem;
  position: relative;
}

.layers {
  width: 100%;
  height: 100vh;
}

@media (max-width: 768px) {
  main {
    flex-direction: column;
  }

  .params {
    width: 100%;
    box-sizing: border-box;
    /* height: 30rem; */
    padding: 1rem;
    z-index: 10;
  }

  .btn {
    display: block;
  }
}
</style>
