<script setup>
import { ref, onMounted } from "vue";
import { NGrid, NGridItem, NCard, NTime, NCol, NRow, NStatistic } from "naive-ui";

const workers = ref([]);

onMounted(async () => {

  let timer = setInterval(() => {
    fetch("/api/workers")
      .then(response => response.json())
      .then(data => {
        workers.value = data;
      })
      .catch(error => console.error("Error fetching workers:", error));
  }, 5000);

  workers.value = await fetch("/api/workers")
    .then(response => response.json())
    .catch(error => console.error("Error fetching workers:", error));

  return () => {
    clearInterval(timer);
  };
});

</script>

<template>
  <n-grid cols="1 600:2">
    <n-grid-item v-for="worker in workers.data" :key="worker.id">
      <n-card size="small" :title="'🟢 ' + worker.name">
        <n-grid cols="2" y-gap="10">
          <n-grid-item>
            <n-statistic label="首次连接">
              <n-time :time="worker.connected_at" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="最新连接">
              <n-time :time="worker.last_ping" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="处理任务量">
              {{ worker.tasks_done }}
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="平均耗时">
              {{ worker.avg_det_time }} s
            </n-statistic>
          </n-grid-item>
        </n-grid>
      </n-card>
    </n-grid-item>
  </n-grid>
</template>
