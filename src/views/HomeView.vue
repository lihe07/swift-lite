<script setup>
import {
  NCard,
  NH1,
  NButton,
  NIcon,
  NUpload,
  NP,
  NText,
  NUploadDragger,
  NScrollbar,
} from "naive-ui";
import {
  WeatherMoon20Filled,
  WeatherSunny20Filled,
  Image28Regular,
} from "@vicons/fluent";
import { inject } from "vue";
import RecentList from "@/components/RecentList.vue";
import ChangeLog from "@/components/ChangeLog.vue";
import { useRouter } from "vue-router";

const isDark = inject("isDark");
const router = useRouter();

function handleFinish({ event }) {
  router.push("/editor/" + JSON.parse(event.target.response).id);
}

function handleError({ event }) {
  console.log("error", event);
}


</script>

<template>
  <n-scrollbar style="max-height: 100vh">
    <main>
      <div class="header">
        <n-h1>Swift Lite</n-h1>

        <n-button round @click="isDark = !isDark">
          <template #icon>
            <n-icon>
              <WeatherMoon20Filled v-if="!isDark" />
              <WeatherSunny20Filled v-else />
            </n-icon>
          </template>
        </n-button>
      </div>

      <n-card title="新的检测">
        <n-upload action="/api/detections" @finish="handleFinish" @error="handleError">
          <n-upload-dragger>
            <div style="margin: 12px 0">
              <n-icon size="48" :depth="3">
                <Image28Regular />
              </n-icon>
            </div>
            <n-text style="font-size: 16px">
              点击或者拖动图片到该区域上传
            </n-text>
            <n-p depth="3" style="margin: 12px 0">
              支持 JPEGs, BMP, PNG, TIFF 格式.

              <br />
              所有上传的图片都会<b>公开显示</b>, 如误传请尽快手动删除.
            </n-p>
          </n-upload-dragger>
        </n-upload>
      </n-card>

      <div class="space"></div>

      <n-card title="最近的检测">
        <RecentList></RecentList>
      </n-card>

      <div class="space"></div>

      <n-card title="更新日志">
        <ChangeLog></ChangeLog>
      </n-card>

    </main>
  </n-scrollbar>
</template>

<style scoped>
main {
  max-width: 50rem;
  margin: auto;
  padding: 3rem 1rem;
}

@media (max-width: 768px) {
  main {
    padding-top: 2rem;
  }
}

.header {
  display: flex;
  justify-content: space-between;
  /* margin-bottom: 1rem; */
}

.space {
  height: 1rem;
}
</style>
