<script setup>
import { RouterView } from "vue-router";
import {
  NConfigProvider,
  NMessageProvider,
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

const themeVars = useThemeVars();
</script>

<template>
  <n-config-provider :theme="isDark ? darkTheme : lightTheme">
    <n-message-provider>
      <n-el
        tag="div"
        style="
          background: var(--body-color);
          transition: background-color 0.3s var(--cubic-bezier-ease-in-out);
        "
        class="body"
      >
        <RouterView></RouterView>
      </n-el>
    </n-message-provider>
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
