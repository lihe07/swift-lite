<script setup>
import { onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  NPageHeader,
  NButton,
  NSpin,
  NText,
  NCard,
  NProgress,
  NEl,
  NFormItem,
  NInput,
  NH3,
  NIcon,
  NCollapseTransition,
  NNumberAnimation,
  useMessage,
} from "naive-ui";
import ParamsForm from "@/components/ParamsForm.vue";
import { ArrowDown16Filled } from "@vicons/fluent";
import OpenSeadragon from "openseadragon";

const route = useRoute();
const router = useRouter();

const data = ref({});
const loading = ref(true);
const progress = ref(null);

async function load() {
  const res = await (await fetch(`/api/detections/${route.params.id}`)).json();
  if (res.error) {
    router.push("/404");
    return;
  }
  res.params = JSON.parse(res.params);
  data.value = res;
  progress.value = res.progress;
  loading.value = false;

  if (res.progress != 100 || res.num == null) {
    // Not finished
    const interval = setInterval(async () => {
      const res = await (
        await fetch(`/api/detections/${route.params.id}`)
      ).json();
      if (res.error) {
        router.push("/404");
        return;
      }
      try {
        res.params = JSON.parse(res.params);
      } catch {
        res.params = {};
      }
      data.value = res;
      progress.value = res.progress;
      if (res.progress == 100 && res.num != null) {
        clearInterval(interval);
      }
    }, 1500);
  }
}

load();

const showForms = ref(false);

let viewer;
function initOpenseagragon(el) {
  viewer = OpenSeadragon({
    element: el,
    prefixUrl:
      "https://cdn.jsdelivr.net/npm/openseadragon@4.0/build/openseadragon/images/",
    tileSources:
      "/api/detections/" +
      route.params.id +
      "/boxes.dzi?t=" +
      data.value.modified_at,
  });
}

function onResize() {
  if (window.innerWidth > 768) {
    showForms.value = true;
  }
}

const message = useMessage();

let lastRemark = null;
let updating = false;

onMounted(() => {
  window.addEventListener("resize", onResize);
  onResize();

  const updator = setInterval(() => {
    const remark = data.value.remark;
    if (lastRemark !== remark) {
      if (lastRemark === null) {
        lastRemark = remark;
        return;
      }
      lastRemark = remark;
      const msg = message.loading("正在更新备注...");
      const t0 = Date.now();

      fetch("/api/detections/" + route.params.id + "/remark", {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(remark),
      }).then(() => {
        setTimeout(msg.destroy, Math.max(0, 1000 - (Date.now() - t0)));
      });
    }
  }, 1000);

  return () => {
    window.removeEventListener("resize", onResize);
    clearInterval(updator);
  };
});

let anim = ref(null);
const lastNumber = ref(0);

function updateParams() {
  const params = data.value.params;

  console.log("update params", params);
  viewer.close();

  fetch("/api/detections/" + route.params.id + "/params", {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(params),
  }).then((res) => {
    if (res.status === 200) {
      res.json().then((res) => {
        console.log("ok");
        console.log(res.num);
        data.value.num = res.num;
        // Reload viewer
        viewer.open(
          "/api/detections/" +
            route.params.id +
            "/boxes.dzi?t=" +
            res.modified_at
        );
        updating = false;
        anim.value.play();
      });
    } else if (res.status === 202) {
      setTimeout(load, 1000);
    }
  });
}
</script>

<template>
  <div>
    <main v-if="!loading && progress === 100 && data.num != null">
      <n-el
        tag="div"
        class="params"
        style="
          background-color: var(--card-color);
          transition: background-color 0.3s var(--cubic-bezier-ease-in-out);
        "
      >
        <n-page-header
          @back="router.push('/')"
          title="Swift Lite"
          subtitle="编辑器"
        >
          <template #extra>
            <!-- <n-button>刷新</n-button> -->
            <n-h3 style="margin: 0">
              <n-number-animation
                ref="anim"
                :from="lastNumber"
                :to="data.num"
                @finish="lastNumber = data.num"
              ></n-number-animation>
              只
            </n-h3>
          </template>
        </n-page-header>

        <n-collapse-transition :show="showForms">
          <div class="forms">
            <ParamsForm :data="data.params"></ParamsForm>

            <n-button class="full-width" @click="updateParams()"
              >重新检测</n-button
            >

            <n-form-item label="备注">
              <n-input v-model:value="data.remark" placeholder="无"></n-input>
            </n-form-item>
          </div>
        </n-collapse-transition>

        <div class="btn">
          <n-button circle secondary @click="showForms = !showForms">
            <template #icon>
              <n-icon>
                <ArrowDown16Filled
                  :style="{
                    transform: showForms ? 'rotate(180deg)' : 'rotate(0)',
                    transition: 'transform 0.3s',
                  }"
                />
              </n-icon>
            </template>
          </n-button>
        </div>
      </n-el>

      <div class="image">
        <div class="layers" :ref="initOpenseagragon"></div>
      </div>
    </main>
    <main v-else class="status">
      <!-- Status -->
      <n-spin :show="loading">
        <template #description>
          <n-text depth="3">如果长时间未加载，请刷新页面</n-text>
        </template>
        <n-card
          class="status-card"
          style="text-align: center; padding: 1rem 1.5rem"
          :style="{ opacity: loading ? 0 : 1 }"
        >
          <n-progress
            type="circle"
            :percentage="progress"
            :status="progress === null ? 'info' : 'default'"
            :stroke-width="8"
          ></n-progress>

          <div style="margin-top: 1rem">
            <n-text v-if="progress > 0">检测中。通常不会超过15s</n-text>
            <n-text v-else>正在排队。您无需保持该页面常开</n-text>
          </div>
        </n-card>
      </n-spin>
    </main>
  </div>
</template>

<style scoped>
main {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.status {
  justify-content: center;
  align-items: center;
}

.params {
  /* flex: 1; */
  width: 30rem;
  padding: 1rem;
  position: relative;
  z-index: 5;
  background: var(--);
}

.full-width {
  width: 100%;
  margin-bottom: 1rem;
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
}

.layers {
  position: relative;
  width: 100%;
  height: 100%;
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
