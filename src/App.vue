<script setup>
import { RouterView } from "vue-router";
import {
  NConfigProvider,
  darkTheme,
  lightTheme,
  useThemeVars,
  NEl,
} from "naive-ui";
import { provide, ref, watch } from "vue";

let defaultDark;
if (["dark", "light"].includes(localStorage.getItem("theme"))) {
  defaultDark = localStorage.getItem("theme") === "dark";
} else {
  defaultDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
}

const isDark = ref(defaultDark);

provide("isDark", isDark);

watch(isDark, (val) => {
  localStorage.setItem("theme", val ? "dark" : "light");
});


const lightThemeOverrides = {
  common: {
    overlayColor: "rgba(255, 255, 255, 0.7)",
  },
}

const darkThemeOverrides = {
  common: {
    overlayColor: "rgba(0, 0, 0, 0.5)",
  }
}

</script>

<template>
  <n-config-provider :theme="isDark ? darkTheme : lightTheme"
    :theme-overrides="isDark ? darkThemeOverrides : lightThemeOverrides">
    <n-el tag="div" style="
        background: var(--body-color);
        transition: background-color 0.3s var(--cubic-bezier-ease-in-out);
      " class="body">
      <RouterView></RouterView>
    </n-el>
  </n-config-provider>
</template>

<style scoped>
.body {
  min-height: 100vh;
}
</style>

<style>
body {
  margin: 0;
}
</style>
