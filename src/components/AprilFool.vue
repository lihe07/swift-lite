<script setup>
import {
  NCard,
  NH1,
  NA,
  NButton,
  NGridItem,
  NImage,
  NGrid,
  NP,
  NText,
} from "naive-ui";
import { ref } from "vue";

import yb1 from "@/assets/april/yb1.png";
import yb2 from "@/assets/april/yb2.png";
import yb3 from "@/assets/april/yb3.png";
import yb4 from "@/assets/april/yb4.png";
import pw1 from "@/assets/april/pw1.png";
import pw2 from "@/assets/april/pw2.png";
import pw3 from "@/assets/april/pw3.png";
import pw4 from "@/assets/april/pw4.png";
import o1 from "@/assets/april/o1.png";
import o2 from "@/assets/april/o2.png";
import o3 from "@/assets/april/o3.png";
import o4 from "@/assets/april/o4.png";

import ConfettiExplosion from "vue-confetti-explosion";

const allImages = [
  {
    src: yb1,
    answer: true,
  },
  {
    src: yb2,
    answer: true,
  },
  {
    src: yb3,
    answer: true,
  },
  {
    src: yb4,
    answer: true,
  },
  {
    src: pw1,
    answer: true,
  },
  {
    src: pw2,
    answer: false,
  },
  {
    src: pw3,
    answer: false,
  },
  {
    src: pw4,
    answer: false,
  },
  {
    src: o1,
    answer: false,
  },
  {
    src: o2,
    answer: false,
  },
  {
    src: o3,
    answer: false,
  },
  {
    src: o4,
    answer: false,
  },
].map((image, index) => ({
  ...image,
  id: index,
}));

const images = allImages.sort(() => Math.random() - 0.5).slice(0, 4);

const selected = ref([]);
const result = ref(null);

function toggle(id) {
  const index = selected.value.indexOf(id);
  if (index === -1) {
    selected.value.push(id);
  } else {
    selected.value.splice(index, 1);
  }
}

async function submit() {
  const correct = images
    .filter((image) => image.answer)
    .every((image) => selected.value.includes(image.id));


  let count
  if (correct) {
    const resp = await fetch("/api/april-fool", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        correct,
      }),
    });

    const j = await resp.json();
    count = j.count;

  }

  result.value = {
    correct,
    count
  }
}

let showAprilFool = ref(true);

// Check session storage has been set
if (sessionStorage.getItem("hide-april-fool")) {
  showAprilFool.value = false;
}

function handleClose() {
  showAprilFool.value = false;
  // sessionStorage.setItem("hide-april-fool", "true");
}


</script>

<template>
  <n-card title="重要通知! - 2024年4月" closable @close="handleClose" v-if="showAprilFool">

    <n-text>
      由于工具的流量和用户数量增长，检测排队等候时间变长。为了避免自动化工具，爬虫等恶意请求影响正常用户，今日起，用户上传照片检测前，需完成图像验证码，由于懒得做会话存储，刷新页面后<b>需要重新完成验证码</b>。
    </n-text>

    <div style="height: 0.5rem;" />

    <n-text>
      此策略自<b>2024年4月1日</b>起不生效，感谢您的理解和支持！图像验证码示例如下：
    </n-text>



    <n-card style="margin-top: 1rem">

      <n-text :depth="2">需要验证！请选择所有的</n-text>
      <p style="font-size: 1.5rem; line-height:1.5rem; margin: 0.5rem 0;">
        <b style="margin-right: 0.3rem;">黄眉柳莺</b>
        <div class="on-mobile" />
        <i style="font-size: 1rem;">Phylloscopus inornatus</i>
      </p>

      <div style="position: relative;">
        <n-grid cols="2 400:4 " x-gap="10" y-gap="10">
          <n-grid-item v-for="image in  images " :key="image.id" style="cursor: pointer;"
            @click="() => toggle(image.id)">

            <img :src="image.src" style="object-fit: cover; border-radius: 0.5rem;" width="100%" height="100%"
              :class="selected.includes(image.id) ? 'selected' : ''" </n-grid-item>
        </n-grid>

        <div class='result' :class="{ show: result }">
          <p v-if="result?.correct" style="display: flex; flex-direction: column; align-items: center;">
            <span>验证通过！</span>
            <ConfettiExplosion :shouldDestroyAfterDone="false" />
            您是第 {{ parseInt(result.count) }} 位通过验证的用户。
          </p>
          <p v-else>
            <span>🔥 验证失败！</span>

            <div />

            <n-button @click="result = null" style="margin-top: 1rem;">再试一次</n-button>

          </p>

        </div>


      </div>


      <template #action>
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <n-button type="primary" @click="submit">提交</n-button>


          <div>
            看不清？<n-button style="margin-left: 0.5rem">再看一眼</n-button>
          </div>
        </div>
      </template>

    </n-card>
  </n-card>
</template>

<style scoped>
.on-mobile {
  display: block;
}

@media (min-width: 768px) {
  .on-mobile {
    display: none;
  }

}

img {
  box-sizing: border-box;
  transition: all 0.2s;
  border: 4px solid transparent;
}

.selected {
  border-color: #63e2b7;
  transform: scale(0.98);
  opacity: 0.8;
}

.result {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
  color: #fff;
  background-color: rgba(0, 0, 0, 0.5);
  border-radius: 0.5rem;

  pointer-events: none;
  transition: all 0.2s;
  opacity: 0;
}

.result span {
  font-size: 1.5rem;
  font-weight: bold;
}

.result p {

  text-align: center;
}

.result.show {
  opacity: 1;
  pointer-events: auto;
}
</style>
