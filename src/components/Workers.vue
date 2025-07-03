<script setup>
import { ref, onMounted } from "vue";
import { NGrid, NGridItem, NCard, NTime, NCol, NRow, NStatistic } from "naive-ui";

const workers = ref([]);
const now = ref(Date.now());

onMounted(async () => {

  let timer = setInterval(() => {
    fetch("/api/workers")
      .then(response => response.json())
      .then(data => {
        workers.value = data;
        now.value = Date.now();
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
  <div v-if="workers.data && workers.data.length === 0" style="text-align: center;">

    <h3>🔥
      无计算节点，服务中止！
    </h3>
  </div>
  <n-grid cols="1 600:2" x-gap="10" y-gap="10" v-if="workers.data">
    <n-grid-item v-for="worker in workers.data" :key="worker.id" v-if="workers.data">
      <n-card size="small" :title="'🟢 ' + worker.name">
        <n-grid cols="2" y-gap="10">
          <n-grid-item>
            <n-statistic label="首次连接">
              <n-time :time="worker.connected_at" :to="now / 1000" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="最新连接">
              <n-time :time="worker.last_ping" :to="now / 1000" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="处理任务量">
              {{ worker.tasks_done }}
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="平均耗时">
              {{ worker.avg_det_time.toFixed(2) }} 秒
            </n-statistic>
          </n-grid-item>
        </n-grid>
      </n-card>
    </n-grid-item>
  </n-grid>
</template>
