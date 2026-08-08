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
  createDiscreteApi,
} from "naive-ui";
import {
  WeatherMoon20Filled,
  WeatherSunny20Filled,
  Image28Regular,
} from "@vicons/fluent";
import { inject, ref, onMounted } from "vue";
import RecentList from "@/components/RecentList.vue";
import Workers from "@/components/Workers.vue";
import ChangeLog from "@/components/ChangeLog.vue";
import AprilFool from "@/components/AprilFool.vue";
import { useRouter } from "vue-router";

const isDark = inject("isDark");
const router = useRouter();

const errorText = ref("");
const showCopyBtn = ref(false);
const copySuccess = ref(false);

function handleFinish({ event }) {
  router.push("/editor/" + JSON.parse(event.target.response).id);
}

function handleError({ event }) {
  console.log("error", event);
  const xhr = event?.target;
  if (xhr) {
    errorText.value = `HTTP ${xhr.status}: ${xhr.responseText || xhr.statusText || "Unknown Error"}`;
  } else {
    errorText.value = "Network Request Failed";
  }
  showCopyBtn.value = true;
  copySuccess.value = false;
}

function copyError() {
  navigator.clipboard.writeText(errorText.value).then(() => {
    copySuccess.value = true;
    setTimeout(() => {
      copySuccess.value = false;
    }, 2000);
  });
}

onMounted(() => {
  if (document.cookie.includes("browser_check_passed=1")) {
    return;
  }

  const unsupportedFeatures = [];

  if (!(window.File && window.FileReader && window.FileList && window.Blob)) {
    unsupportedFeatures.push("文件上传");
  }

  if (!('ondragstart' in window && 'ondrop' in window)) {
    unsupportedFeatures.push("文件拖拽");
  }

  if (!(navigator.clipboard && navigator.clipboard.writeText)) {
    unsupportedFeatures.push("剪贴板复制");
  }

  if (typeof window.Promise !== "function" || typeof window.fetch !== "function") {
    unsupportedFeatures.push("现代网络请求");
  }

  if (unsupportedFeatures.length === 0) {
    document.cookie = "browser_check_passed=1; max-age=2592000; path=/";
  } else {
    const { dialog } = createDiscreteApi(["dialog"]);
    const featureText = unsupportedFeatures.join("、");
    dialog.warning({
      title: "浏览器兼容性提示",
      content: `该浏览器部分${featureText}功能不支持是否继续访问建议切换使用edge或chrome 夸克浏览器`,
      positiveText: "继续访问",
      negativeText: "切换浏览器",
      maskClosable: false,
      onPositiveClick: () => {
        document.cookie = "browser_check_passed=1; max-age=86400; path=/";
      }
    });
  }
});
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

        <div v-if="showCopyBtn" style="margin-top: 16px; display: flex; justify-content: center;">
          <n-button size="small" type="error" ghost @click="copyError">
            {{ copySuccess ? "已复制" : "复制错误信息" }}
          </n-button>
        </div>
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
}

.space {
  height: 1rem;
}
