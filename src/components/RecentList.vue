<script setup>
import { NDataTable, NButtonGroup, NButton, NText } from "naive-ui";
import { useRouter } from "vue-router";
import { h, ref } from "vue";

const router = useRouter();

const columns = [
  {
    title: "修改时间",
    key: "modified_at",
    render(row) {
      // Parse Unix timestamp to human readable string
      return new Date(row.modified_at * 1000).toLocaleString();
    },
  },
  {
    title: "数量",
    key: "num",
  },
  {
    title: "备注",
    key: "remark",
    default: "无",
  },
  {
    title: "操作",
    key: "actions",
    render(row) {
      return [
        h(
          NButton,
          {
            type: "default",
            size: "small",
            style: "margin-right: 5px",
            onClick: () => router.push(`/editor/${row.id}`),
          },
          () => "编辑"
        ),
        h(
          NButton,
          {
            type: "error",
            size: "small",
            onClick: () => deleteDetection(row.id),
          },
          () => "删除"
        ),
      ];
    },
  },
];

function renderCell(value) {
  if (!value) {
    return h(NText, { depth: 3 }, { default: () => "无" });
  }
  return value;
}

const data = ref([]);

const loading = ref(true);

function fetchData() {
  loading.value = true;
  fetch("/api/detections")
    .catch((err) => {
      console.error(err);
      loading.value = false;
    })
    .then((resp) => resp.json())
    .then((resp) => {
      console.log(resp);
      if (resp.length) data.value = resp;
      else data.value = [];
      loading.value = false;
    });
}

fetchData();

function deleteDetection(id) {
  fetch(`/api/detections/${id}`, {
    method: "DELETE",
  }).then(fetchData);
}
</script>

<template>
  <n-data-table
    :columns="columns"
    :data="data"
    :render-cell="renderCell"
    :loading="loading"
  ></n-data-table>
</template>
