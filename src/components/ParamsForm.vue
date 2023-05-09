<script setup>
import { NForm, NFormItem, NSwitch, NSlider, NInputNumber } from "naive-ui";

const props = defineProps(["data"]);
</script>

<template>
  <div>
    <n-form-item label="是否为纯天空">
      <n-switch v-model:value="props.data.is_pure_sky"></n-switch>
    </n-form-item>

    <n-form :disabled="props.data.is_pure_sky">
      <n-form-item label="天空检测阈值">
        <n-input-number
          style="width: 7rem; margin-right: 1rem"
          :show-button="false"
          v-model:value="props.data.sky_threshold"
          :step="0.01"
          :min="0"
          :max="1"
        ></n-input-number>

        <n-slider
          v-model:value="props.data.sky_threshold"
          :step="0.01"
          :min="0"
          :max="1"
        ></n-slider>
      </n-form-item>

      <n-form-item label="天空侵蚀">
        <n-input-number
          style="width: 7rem; margin-right: 1rem"
          :show-button="false"
          v-model:value="props.data.sky_erode"
          :step="1"
          :min="0"
          :max="300"
        >
        </n-input-number>
        <n-slider
          v-model:value="props.data.sky_erode"
          :step="1"
          :min="0"
          :max="300"
        >
        </n-slider>
      </n-form-item>
    </n-form>

    <n-form-item label="雨燕检测阈值">
      <n-input-number
        style="width: 7rem; margin-right: 1rem"
        :show-button="false"
        v-model:value="props.data.point_threshold"
        :step="0.01"
        :min="0"
        :max="1"
      ></n-input-number>

      <n-slider
        v-model:value="props.data.point_threshold"
        :step="0.01"
        :min="0"
        :max="1"
      ></n-slider>
    </n-form-item>
  </div>
</template>

<style scoped></style>
