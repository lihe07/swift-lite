<script setup>
import { NSlider } from "naive-ui";

import { defineProps, defineEmits, ref } from "vue";

const props = defineProps(["value", "step", "min", "max"])

const emit = defineEmits(["update:value"])

const localValue = ref(props.value)

</script>

<template>
  <n-slider v-model:value="localValue" :step="props.step" :min="props.min" :max="props.max"
    @dragend="() => emit('update:value', localValue)"></n-slider>
</template>
