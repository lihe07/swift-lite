<script setup>
import {
  NCard,
  NH1,
  NA,
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
import Workers from "@/components/Workers.vue";
import ChangeLog from "@/components/ChangeLog.vue";
import AprilFool from "@/components/AprilFool.vue";
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
        <n-h1>
          Swift Lite
        </n-h1>

        <n-button round @click="isDark = !isDark">
          <template #icon>
            <n-icon>
              <WeatherMoon20Filled v-if="!isDark" />
              <WeatherSunny20Filled v-else />
            </n-icon>
          </template>
        </n-button>
      </div>

      <AprilFool />
      <div class="space"></div>

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

      <n-card title="计算节点">
        <template #header-extra>
          机器闲置？
          <n-a href="https://www.harvey-l.com/contact/" target="_blank">加入计算</n-a>
        </template>
        <Workers></Workers>
      </n-card>

      <div class="space"></div>

      <n-card title="检测列表">
        <RecentList></RecentList>
      </n-card>

      <div class="space"></div>

      <n-card title="更新日志">
        <ChangeLog></ChangeLog>
      </n-card>


      <div class="space"></div>


      <div style="text-align: center;">
        <n-text depth="3">
          &copy; 2021 - {{ new Date().getFullYear() }} Swift Lite by <n-a style="opacity: 0.5"
            href="https://www.harvey-l.com" target="_blank">
            He Li
          </n-a>
        </n-text>
      </div>

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
