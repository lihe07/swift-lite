<script setup>
import { NForm, NFormItem, NSwitch, NSlider, NInputNumber } from "naive-ui";

const props = defineProps(["data"]);
</script>

<template>
  <div>
    <n-form-item label="雨燕检测阈值">
      <n-input-number
        style="width: 7rem; margin-right: 1rem"
        :show-button="false"
        v-model:value="props.data.threshold"
        :step="0.01"
        :min="0"
        :max="1"
      ></n-input-number>

      <n-slider
        v-model:value="props.data.threshold"
        :step="0.01"
        :min="0"
        :max="1"
      ></n-slider>
    </n-form-item>

    <n-form-item label="滑动窗口大小">
      <n-input-number
        style="width: 7rem; margin-right: 1rem"
        :show-button="false"
        v-model:value="props.data.window_size"
        :step="10"
        :min="600"
        :max="2000"
      ></n-input-number>

      <n-slider
        v-model:value="props.data.window_size"
        :step="10"
        :min="600"
        :max="2000"
      ></n-slider>
    </n-form-item>
  </div>
</template>

<style scoped></style>
