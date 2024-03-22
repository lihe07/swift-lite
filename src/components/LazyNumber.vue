<script setup>
import { NInputNumber } from "naive-ui";

import { defineProps, defineEmits, ref, watchEffect } from "vue";

const props = defineProps(["value", "step", "min", "max"])

const emit = defineEmits(["update:value"])

const localValue = ref(props.value)

watchEffect(() => {
  localValue.value = props.value
})

function checkAndUpdate() {
  // If localValue.value is not a number, set it to props.value
  if (isNaN(parseFloat(localValue.value))) {
    localValue.value = props.value
  }
  emit('update:value', localValue.value)
}

</script>

<template>
  <n-input-number v-model:value="localValue" :step="props.step" :min="props.min" :max="props.max" :show-button="false"
    @blur="checkAndUpdate"></n-input-number>
</template>
