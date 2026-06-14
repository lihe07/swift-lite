<script setup>
import { ref, onMounted, computed } from "vue";
import { NGrid, NGridItem, NCard, NTime, NStatistic, NTag, NSpace } from "naive-ui";

const workers = ref({ data: [] });
const now = ref(Date.now());

async function refresh() {
  try {
    workers.value = await fetch("/api/workers").then((r) => r.json());
    now.value = Date.now();
  } catch (e) {
    console.error("Error fetching workers:", e);
  }
}

const list = computed(() => (workers.value && workers.value.data) || []);

/** tasks per hour over the worker's uptime (connected_at -> last_ping). */
function throughput(w) {
  const uptime = Math.max(1, (w.last_ping || 0) - (w.connected_at || 0));
  return ((w.tasks_done || 0) / uptime) * 3600;
}

onMounted(() => {
  refresh();
  const timer = setInterval(refresh, 5000);
  return () => clearInterval(timer);
});
</script>

<template>
  <div v-if="list.length === 0" style="text-align: center">
    <h3>🔥 无计算节点，服务中止！</h3>
  </div>

  <n-grid v-else cols="1 600:2" x-gap="10" y-gap="10">
    <n-grid-item v-for="worker in list" :key="worker.id">
      <n-card size="small">
        <template #header>
          <n-space align="center" :size="8">
            <n-tag type="success" size="small" round>● 在线</n-tag>
            <span>{{ worker.name }}</span>
          </n-space>
        </template>

        <n-grid cols="2" y-gap="12" x-gap="8">
          <n-grid-item>
            <n-statistic label="首次连接">
              <n-time :time="worker.connected_at" :to="now / 1000" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="最近活动">
              <n-time :time="worker.last_ping" :to="now / 1000" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="处理任务量">
              {{ worker.tasks_done ?? 0 }}
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="平均耗时">
              {{ (worker.avg_det_time ?? 0).toFixed(2) }} 秒
            </n-statistic>
          </n-grid-item>
          <n-grid-item :span="2">
            <n-statistic label="吞吐量">
              {{ throughput(worker).toFixed(1) }} 张 / 小时
            </n-statistic>
          </n-grid-item>
        </n-grid>
      </n-card>
    </n-grid-item>
  </n-grid>
</template>
