<script setup>
import { NForm, NFormItem, NSwitch, NSlider, NInputNumber, NText } from "naive-ui";
import LazySlider from "./LazySlider.vue";

const props = defineProps(["data"]);

function toggleTiling(e) {
  console.log(e);
}

</script>

<template>
  <div>

    <n-text tag="h2" depth="2" class="hide-on-mobile">检测器参数</n-text>

    <n-form-item label="是否分块检测">
      <n-switch v-model:value="props.data.tiling"></n-switch>
    </n-form-item>

    <n-form :disabled="!props.data.tiling">
      <n-form-item label="窗口大小">
        <n-input-number style="width: 7rem; margin-right: 1rem" :show-button="false"
          v-model:value="props.data.window_size" :step="0.01" :min="0" :max="1"></n-input-number>

        <LazySlider v-model:value="props.data.window_size" :step="0.01" :min="0.5" :max="1"></LazySlider>
      </n-form-item>

      <n-form-item label="重叠比例">
        <n-input-number style="width: 7rem; margin-right: 1rem" :show-button="false" v-model:value="props.data.overlap"
          :step="0.01" :min="0" :max="1">
        </n-input-number>
        <LazySlider v-model:value="props.data.overlap" :step="0.01" :min="0" :max="1">
        </LazySlider>
      </n-form-item>
    </n-form>

    <n-text tag="h2" depth="2" class="hide-on-mobile">过滤参数</n-text>

    <n-form-item label="置信阈值">
      <n-input-number style="width: 7rem; margin-right: 1rem" :show-button="false" v-model:value="props.data.threshold"
        :step="0.01" :min="0.05" :max="1"></n-input-number>

      <LazySlider v-model:value="props.data.threshold" :step="0.01" :min="0.05" :max="1"></LazySlider>
    </n-form-item>

    <n-form-item label="重叠阈值">
      <n-input-number style="width: 7rem; margin-right: 1rem" :show-button="false" v-model:value="props.data.iou"
        :step="0.01" :min="0" :max="0.95"></n-input-number>

      <LazySlider v-model:value="props.data.iou" :step="0.01" :min="0" :max="1"></LazySlider>
    </n-form-item>
  </div>
</template>

<style scoped>
.hide-on-mobile {
  display: none;
}

@media (min-width: 768px) {
  .hide-on-mobile {
    display: block;
  }
}
</style>
