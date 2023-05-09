<script setup>
import { onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  NPageHeader,
  NButton,
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

const data = ref({});
const loading = ref(true);

fetch(`/api/detections/${route.params.id}`)
  .then((res) => res.json())
  .then((res) => {
    if (res.error) {
      router.push("/404");
      return;
    }
    data.value = res;
    loading.value = false;
  });

const showForms = ref(false);

let viewer;
function initOpenseagragon(el) {
  viewer = OpenSeadragon({
    element: el,
    prefixUrl:
      "https://cdn.jsdelivr.net/npm/openseadragon@4.0/build/openseadragon/images/",
    tileSources: "/api/detections/" + route.params.id + "/merged.dzi",
  });
}

function onResize() {
  if (window.innerWidth > 768) {
    showForms.value = true;
  }
}

let lastParams = null;
let lastRemark = null;
let updating = false;

onMounted(() => {
  window.addEventListener("resize", onResize);
  onResize();

  const updator = setInterval(() => {
    const params = JSON.stringify(data.value.params);
    if (lastParams !== params) {
      if (lastParams === null) {
        lastParams = params;
        return;
      }
      lastParams = params;
      if (!updating) {
        updating = true;
        updateParams(data.value.params);
      }
    }

    const remark = data.value.remark;
    if (lastRemark !== remark) {
      if (lastRemark === null) {
        lastRemark = remark;
        return;
      }
      lastRemark = remark;
      fetch("/api/detections/" + route.params.id + "/remark", {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ remark }),
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

function updateParams(params) {
  console.log("update params", params);
  viewer.close();
  fetch("/api/detections/" + route.params.id + "/params", {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(params),
  })
    .then((res) => res.json())
    .then((res) => {
      console.log("ok");
      console.log(res.num);
      data.value.num = res.num;
      // Reload viewer
      viewer.open(
        "/api/detections/" + route.params.id + "/merged.dzi?t=" + Date.now()
      );
      updating = false;
      anim.value.play();
    });
}
</script>

<template>
  <n-spin :show="loading">
    <template #description>
      <n-text depth="3">如果长时间未加载，请刷新页面</n-text>
    </template>
    <main v-if="!loading">
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
